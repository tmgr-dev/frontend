import { isKnownEvent, validProps, type EventName, type TrackArgs } from './catalog';
import {
	buildEnvelope,
	hourStart,
	MAX_AGE_MS,
	MAX_QUEUE,
	pickBatch,
	type EnvelopeMeta,
	type QueuedEvent,
} from './envelope';
import { createIdentityStore, type Identity } from './identity';

export const QUEUE_KEY = 'tmgr:telemetry:queue';
export const FLUSH_INTERVAL_MS = 30_000;
const BACKOFF_BASE_MS = 30_000;
const BACKOFF_MAX_MS = 15 * 60_000;

export interface ConsentView {
	on: boolean;
	generation: number;
}

export interface ChannelLike {
	postMessage(data: unknown): void;
	onmessage: ((event: never) => void) | null;
}

export interface TelemetryDeps {
	storage: Storage;
	now: () => number;
	fetch: typeof fetch;
	baseUrl: string;
	consent: { read(): ConsentView; deny(): void };
	meta: () => Omit<EnvelopeMeta, 'session_id'>;
	uuid: () => string;
	secret: () => string;
	sha256: (text: string) => Promise<string>;
	setInterval: (fn: () => void, ms: number) => unknown;
	clearInterval: (handle: unknown) => void;
	channel?: ChannelLike | null;
}

export interface TelemetrySnapshot {
	active: boolean;
	installId: string | null;
	pending: number;
}

export interface DeleteResult {
	deleted: number;
	failed: number;
}

export const createTelemetry = (deps: TelemetryDeps) => {
	const identities = createIdentityStore({
		storage: deps.storage,
		now: deps.now,
		uuid: deps.uuid,
		secret: deps.secret,
	});
	const session = deps.uuid();
	const listeners = new Set<() => void>();

	let active = false;
	let epoch = 0;
	let generation = 0;
	let seq = 0;
	let queue: QueuedEvent[] = [];
	let secretHash: { secret: string; hash: string } | null = null;
	let inflight: AbortController | null = null;
	let timer: unknown = null;
	let failures = 0;
	let retryAt = 0;
	let chain: Promise<void> = Promise.resolve();

	const emit = () => listeners.forEach((fn) => fn());

	const persist = () => {
		const ep = epoch;
		const body = JSON.stringify({ generation, events: queue });
		chain = chain.then(() => {
			if (ep !== epoch) return;
			try {
				deps.storage.setItem(QUEUE_KEY, body);
			} catch {
				// queue stays in memory
			}
		});
	};

	const fresh = (events: QueuedEvent[]) =>
		events.filter((e) => e.t * 1000 >= deps.now() - MAX_AGE_MS);

	const loadQueue = (): QueuedEvent[] => {
		try {
			const data = JSON.parse(deps.storage.getItem(QUEUE_KEY) ?? 'null');
			if (data?.generation === generation && Array.isArray(data.events)) {
				return fresh(data.events).slice(-MAX_QUEUE);
			}
		} catch {
			// unreadable queue is discarded
		}
		deps.storage.removeItem(QUEUE_KEY);
		return [];
	};

	const stop = () => {
		epoch += 1;
		active = false;
		queue = [];
		failures = 0;
		retryAt = 0;
		secretHash = null;
		if (timer !== null) deps.clearInterval(timer);
		timer = null;
		inflight?.abort();
		inflight = null;
		deps.storage.removeItem(QUEUE_KEY);
		identities.retire();
		deps.channel?.postMessage('stop');
		emit();
	};

	const start = (gen: number) => {
		const identity = identities.read() ?? identities.create();
		if (!identity) return;
		epoch += 1;
		active = true;
		generation = gen;
		queue = loadQueue();
		timer = deps.setInterval(() => void flush(), FLUSH_INTERVAL_MS);
		emit();
	};

	const sync = () => {
		const consent = deps.consent.read();
		if (!consent.on) {
			if (active || identities.read() || deps.storage.getItem(QUEUE_KEY) !== null) stop();
			return;
		}
		if (active && consent.generation !== generation) stop();
		if (!active) start(consent.generation);
		else if (!identities.read()) identities.create();
	};

	const endpoint = (path: string) => `${deps.baseUrl}${path}`;

	const hashOf = async (identity: Identity) => {
		if (secretHash?.secret !== identity.secret) {
			secretHash = { secret: identity.secret, hash: await deps.sha256(identity.secret) };
		}
		return secretHash.hash;
	};

	const flush = async ({ keepalive = false } = {}): Promise<void> => {
		sync();
		if (!active || inflight) return;
		queue = fresh(queue);
		if (!queue.length || deps.now() < retryAt) return;
		const ep = epoch;
		const controller = new AbortController();
		inflight = controller;
		let again = false;
		try {
			const identity = identities.read();
			if (!identity) return;
			const sha = identity.registered ? undefined : await hashOf(identity);
			if (ep !== epoch) return;
			const consent = deps.consent.read();
			if (!consent.on || consent.generation !== generation) {
				sync();
				return;
			}
			const batch = pickBatch(queue);
			const response = await deps.fetch(endpoint('t/events'), {
				method: 'POST',
				credentials: 'omit',
				cache: 'no-store',
				referrerPolicy: 'no-referrer',
				keepalive,
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify(buildEnvelope(identity.install_id, batch, sha)),
				signal: controller.signal,
			});
			if (ep !== epoch) return;
			if (response.status >= 200 && response.status < 300) {
				const sent = new Set(batch.map((e) => e.id));
				queue = queue.filter((e) => !sent.has(e.id));
				failures = 0;
				retryAt = 0;
				if (!identity.registered) identities.markRegistered(true);
				persist();
				emit();
				again = queue.length > 0;
			} else if (response.status === 429 || response.status >= 500) {
				backoff();
			} else if (identity.registered) {
				identities.markRegistered(false);
				backoff();
			} else {
				const sent = new Set(batch.map((e) => e.id));
				queue = queue.filter((e) => !sent.has(e.id));
				persist();
				emit();
			}
		} catch {
			if (ep === epoch) backoff();
		} finally {
			if (inflight === controller) inflight = null;
		}
		if (again) await flush({ keepalive });
	};

	const backoff = () => {
		failures += 1;
		retryAt = deps.now() + Math.min(BACKOFF_BASE_MS * 2 ** (failures - 1), BACKOFF_MAX_MS);
	};

	const track = <N extends EventName>(name: N, ...args: TrackArgs<N>) => {
		const props = (args[0] ?? {}) as Record<string, unknown>;
		if (!isKnownEvent(name) || !validProps(name, props)) return;
		sync();
		if (!active) return;
		const entry: QueuedEvent = {
			id: deps.uuid(),
			n: name,
			t: hourStart(deps.now()),
			seq: seq++,
			p: props,
			m: { session_id: session, ...deps.meta() },
		};
		queue.push(entry);
		if (queue.length > MAX_QUEUE) queue = queue.slice(-MAX_QUEUE);
		persist();
		emit();
	};

	const deleteSent = async (): Promise<DeleteResult> => {
		if (deps.consent.read().on) deps.consent.deny();
		sync();
		const result: DeleteResult = { deleted: 0, failed: 0 };
		for (const receipt of identities.receipts()) {
			try {
				const response = await deps.fetch(
					endpoint(`t/installs/${encodeURIComponent(receipt.install_id)}`),
					{
						method: 'DELETE',
						credentials: 'omit',
						cache: 'no-store',
						referrerPolicy: 'no-referrer',
						headers: { 'X-Telemetry-Secret': receipt.secret },
					},
				);
				if (response.status === 204 || response.status === 404) {
					identities.removeReceipt(receipt.install_id);
					result.deleted += 1;
				} else result.failed += 1;
			} catch {
				result.failed += 1;
			}
		}
		emit();
		return result;
	};

	const resetId = () => {
		sync();
		if (!active) return;
		inflight?.abort();
		inflight = null;
		epoch += 1;
		secretHash = null;
		identities.rotate();
		persist();
		emit();
	};

	if (deps.channel) deps.channel.onmessage = () => sync();

	return {
		track,
		flush,
		sync,
		deleteSent,
		resetId,
		subscribe(fn: () => void) {
			listeners.add(fn);
			return () => listeners.delete(fn);
		},
		snapshot(): TelemetrySnapshot {
			return { active, installId: identities.read()?.install_id ?? null, pending: queue.length };
		},
		queueJson: () => JSON.stringify(queue.map(({ id, n, t, seq: s, p, m }) => ({ id, n, t, seq: s, p, ...m })), null, 2),
	};
};

export type Telemetry = ReturnType<typeof createTelemetry>;
