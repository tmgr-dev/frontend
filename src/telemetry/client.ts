import {
	copyProps,
	isKnownEvent,
	type EventName,
	type TrackArgs,
} from './catalog';
import {
	buildEnvelope,
	hourStart,
	MAX_AGE_MS,
	MAX_QUEUE,
	normalizeEntry,
	pickBatch,
	sanitizeMeta,
	type EnvelopeMeta,
	type QueuedEvent,
} from './envelope';
import { createIdentityStore, type Identity, type Receipt } from './identity';
import { withLock, type Locks } from './locks';

export const QUEUE_KEY = 'tmgr:telemetry:queue';
export const FLUSH_INTERVAL_MS = 30_000;
const BACKOFF_BASE_MS = 30_000;
const BACKOFF_MAX_MS = 15 * 60_000;

export interface ConsentView {
	on: boolean;
	generation: number;
}

export interface TelemetryDeps {
	storage: Storage | null;
	now: () => number;
	fetch: typeof fetch;
	baseUrl: string;
	consent: { read(): ConsentView; deny(): unknown };
	meta: () => Omit<EnvelopeMeta, 'session_id'>;
	uuid: () => string;
	secret: () => string;
	sha256: (text: string) => Promise<string>;
	setInterval: (fn: () => void, ms: number) => unknown;
	clearInterval: (handle: unknown) => void;
	later: (fn: () => void, ms: number) => void;
	locks?: Locks | null;
}

export interface TelemetrySnapshot {
	active: boolean;
	installId: string | null;
	pending: number;
	error: 'receipt' | null;
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
		locks: deps.locks,
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
	let identityReady: Promise<Identity | null> = Promise.resolve(null);

	const safely = (fn: () => void) => {
		try {
			fn();
		} catch {}
	};
	let error: 'receipt' | null = null;
	const noop = () => {};

	const emit = () => listeners.forEach((fn) => fn());

	const queueLock = (fn: () => void) => withLock(deps.locks, 'queue', fn);

	const storeQueue = (events: QueuedEvent[]) =>
		safely(() => {
			if (events.length)
				deps.storage?.setItem(QUEUE_KEY, JSON.stringify({ events }));
			else deps.storage?.removeItem(QUEUE_KEY);
		});

	const persist = () => {
		const ep = epoch;
		const snapshot = queue.slice();
		const write = () => {
			if (ep !== epoch) return;
			const consent = deps.consent.read();
			if (!consent.on) return;
			storeQueue(snapshot.filter((e) => e.g === consent.generation));
		};
		chain = chain.then(() => queueLock(write)).then(noop, noop);
	};

	const fresh = (events: QueuedEvent[]) =>
		events.filter((e) => e.t * 1000 >= deps.now() - MAX_AGE_MS);

	const readStoredQueue = (): QueuedEvent[] => {
		try {
			const data = JSON.parse(deps.storage?.getItem(QUEUE_KEY) ?? 'null');
			return Array.isArray(data?.events)
				? data.events
						.map(normalizeEntry)
						.filter((e: QueuedEvent | null): e is QueuedEvent => e !== null)
				: [];
		} catch {
			return [];
		}
	};

	const purgeStored = () => {
		const consent = deps.consent.read();
		if (!consent.on) return safely(() => deps.storage?.removeItem(QUEUE_KEY));
		storeQueue(
			fresh(readStoredQueue()).filter((e) => e.g === consent.generation),
		);
	};

	const loadQueue = (gen: number): QueuedEvent[] => {
		const events = fresh(readStoredQueue())
			.filter((e) => e.g === gen)
			.slice(-MAX_QUEUE);
		storeQueue(events);
		return events;
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
		safely(() => deps.storage?.removeItem(QUEUE_KEY));
		chain = chain.then(() => queueLock(purgeStored)).then(noop, noop);
		deps.later(() => void queueLock(purgeStored).then(noop, noop), 1000);
		emit();
	};

	const withdraw = () => {
		stop();
		return identities.retire().then((retired) => {
			error = retired ? null : 'receipt';
			emit();
		});
	};

	const start = (gen: number) => {
		epoch += 1;
		active = true;
		generation = gen;
		queue = loadQueue(gen);
		const ep = epoch;
		identityReady = identities
			.create(() => {
				const consent = deps.consent.read();
				return ep === epoch && consent.on && consent.generation === gen;
			})
			.catch(() => null);
		timer = deps.setInterval(() => void flush(), FLUSH_INTERVAL_MS);
		emit();
		void identityReady.then((identity) => {
			if (ep !== epoch) return;
			if (!identity) stop();
			else emit();
		});
	};

	const sync = () => {
		const consent = deps.consent.read();
		if (!consent.on) {
			let queued = false;
			try {
				queued = deps.storage?.getItem(QUEUE_KEY) != null;
			} catch {
				queued = false;
			}
			if (active || queued) stop();
			return;
		}
		if (active && consent.generation !== generation) stop();
		if (!active) start(consent.generation);
	};

	const endpoint = (path: string) => `${deps.baseUrl}${path}`;

	const hashOf = async (identity: Identity) => {
		if (secretHash?.secret !== identity.secret) {
			secretHash = {
				secret: identity.secret,
				hash: await deps.sha256(identity.secret),
			};
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
			await identityReady;
			const identity = identities.read();
			if (!identity) return;
			const sha = identity.registered ? undefined : await hashOf(identity);
			if (ep !== epoch) return;
			const sending = await withLock(deps.locks, 'consent', () =>
				withLock(deps.locks, 'send', () =>
					withLock(deps.locks, 'identity', () => {
						const consent = deps.consent.read();
						if (ep !== epoch || !active || controller.signal.aborted)
							return null;
						if (!consent.on || consent.generation !== generation) {
							sync();
							return null;
						}
						const current = identities.read();
						if (
							current?.install_id !== identity.install_id ||
							current.secret !== identity.secret
						)
							return null;
						queue = queue.filter((e) => e.g === consent.generation);
						if (!queue.length) return null;
						const batch = pickBatch(queue);
						const response = deps.fetch(endpoint('t/events'), {
							method: 'POST',
							credentials: 'omit',
							cache: 'no-store',
							referrerPolicy: 'no-referrer',
							keepalive,
							headers: { 'Content-Type': 'application/json' },
							body: JSON.stringify(
								buildEnvelope(identity.install_id, batch, sha),
							),
							signal: controller.signal,
						});
						// The wrapper releases the locks before the network response arrives.
						void response.catch(noop);
						return { response, batch };
					}),
				),
			);
			if (!sending) return;
			const { batch } = sending;
			const response = await sending.response;
			if (ep !== epoch) return;
			if (response.status >= 200 && response.status < 300) {
				const sent = new Set(batch.map((e) => e.id));
				queue = queue.filter((e) => !sent.has(e.id));
				failures = 0;
				retryAt = 0;
				if (!identity.registered)
					await identities.markRegistered(identity.install_id, true);
				if (ep !== epoch) return;
				persist();
				emit();
				again = queue.length > 0;
			} else if (response.status === 429 || response.status >= 500) {
				backoff();
			} else if (identity.registered) {
				await identities.markRegistered(identity.install_id, false);
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
		retryAt =
			deps.now() +
			Math.min(BACKOFF_BASE_MS * 2 ** (failures - 1), BACKOFF_MAX_MS);
	};

	const track = <N extends EventName>(name: N, ...args: TrackArgs<N>) => {
		if (!isKnownEvent(name)) return;
		const props = copyProps(name, (args[0] ?? {}) as Record<string, unknown>);
		const m = sanitizeMeta({ session_id: session, ...deps.meta() });
		if (!props || !m) return;
		sync();
		if (!active) return;
		const entry: QueuedEvent = {
			id: deps.uuid(),
			n: name,
			t: hourStart(deps.now()),
			seq: seq++,
			p: props,
			m,
			g: generation,
		};
		queue.push(entry);
		if (queue.length > MAX_QUEUE) queue = queue.slice(-MAX_QUEUE);
		persist();
		emit();
	};

	const deleteSent = async (): Promise<DeleteResult> => {
		const installId = identities.read()?.install_id;
		stop();
		await deps.consent.deny();
		if (installId)
			error = (await identities.retire(installId)) ? null : 'receipt';
		const result: DeleteResult = { deleted: 0, failed: 0 };
		const targets: Receipt[] = await identities.receipts();
		const leftover = identities.read();
		if (
			leftover &&
			leftover.install_id === installId &&
			!targets.some((r) => r.install_id === installId)
		) {
			targets.push({
				install_id: leftover.install_id,
				secret: leftover.secret,
			});
		}
		for (const receipt of targets) {
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
					await identities.forget(receipt.install_id);
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
		const ep = epoch;
		identityReady = identities
			.rotate(() => ep === epoch && deps.consent.read().on)
			.catch(() => null);
		persist();
		emit();
		return identityReady.then(() => emit());
	};

	void identities.receipts().catch(noop);

	return {
		track,
		flush,
		sync,
		stop,
		withdraw,
		deleteSent,
		resetId,
		subscribe(fn: () => void) {
			listeners.add(fn);
			return () => listeners.delete(fn);
		},
		snapshot(): TelemetrySnapshot {
			return {
				active,
				installId: identities.read()?.install_id ?? null,
				pending: queue.length,
				error,
			};
		},
		queueJson: () =>
			JSON.stringify(
				queue.map(({ id, n, t, seq: s, p, m }) => ({
					id,
					n,
					t,
					seq: s,
					p,
					...m,
				})),
				null,
				2,
			),
	};
};

export type Telemetry = ReturnType<typeof createTelemetry>;
