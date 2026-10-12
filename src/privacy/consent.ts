export type ConsentValue = 'granted' | 'denied' | null;

export interface ConsentRecord {
	analytics: ConsentValue;
	usage: ConsentValue;
	usageGeneration: number;
	updatedAt: string | null;
}

export type ConsentListener = (record: ConsentRecord) => void;

export const CONSENT_KEY = 'tmgr:consent:v1';
export const CONSENT_CHANNEL = 'tmgr-consent';

const empty = (): ConsentRecord => ({
	analytics: null,
	usage: null,
	usageGeneration: 0,
	updatedAt: null,
});

const value = (raw: unknown): ConsentValue =>
	raw === 'granted' || raw === 'denied' ? raw : null;

const parse = (raw: string | null): ConsentRecord => {
	if (!raw) return empty();
	try {
		const data = JSON.parse(raw);
		return {
			analytics: value(data?.analytics),
			usage: value(data?.usage),
			usageGeneration:
				Number.isInteger(data?.usageGeneration) && data.usageGeneration > 0
					? data.usageGeneration
					: 0,
			updatedAt: typeof data?.updatedAt === 'string' ? data.updatedAt : null,
		};
	} catch {
		return empty();
	}
};

export interface ConsentStore {
	get(): ConsentRecord;
	read(): ConsentRecord;
	saveFailed(): boolean;
	set(patch: Partial<Pick<ConsentRecord, 'analytics' | 'usage'>>): ConsentRecord;
	subscribe(listener: ConsentListener): () => void;
}

type Patch = Partial<Pick<ConsentRecord, 'analytics' | 'usage'>>;

const sameState = (a: ConsentRecord, b: ConsentRecord) =>
	a.usage === b.usage && a.usageGeneration === b.usageGeneration && a.analytics === b.analytics;

const merge = (base: ConsentRecord, patch: Patch): ConsentRecord => ({
	...base,
	...(patch.analytics !== undefined && { analytics: patch.analytics }),
	...(patch.usage !== undefined && {
		usage: patch.usage,
		usageGeneration:
			patch.usage === base.usage ? base.usageGeneration : base.usageGeneration + 1,
	}),
	updatedAt: new Date().toISOString(),
});

const usageWinner = (a: ConsentRecord, b: ConsentRecord): ConsentRecord => {
	if (a.usageGeneration !== b.usageGeneration) return a.usageGeneration > b.usageGeneration ? a : b;
	return a.usage === 'denied' ? a : b;
};

export const createConsentStore = (win: Window = window): ConsentStore => {
	const listeners = new Set<ConsentListener>();
	let current: ConsentRecord | null = null;
	let channel: BroadcastChannel | null = null;
	let wired = false;
	let hardOff = false;
	let failed = false;

	const readStored = (): ConsentRecord => {
		try {
			return parse(win.localStorage.getItem(CONSENT_KEY));
		} catch {
			return current ?? empty();
		}
	};

	const read = (): ConsentRecord => {
		const stored = readStored();
		return hardOff && stored.usage === 'granted'
			? { ...stored, usage: 'denied', usageGeneration: stored.usageGeneration + 1 }
			: stored;
	};

	const adopt = (record: ConsentRecord) => {
		current = record;
		listeners.forEach((listener) => listener(record));
	};

	const wire = () => {
		if (wired) return;
		wired = true;
		win.addEventListener('storage', (event: StorageEvent) => {
			if (event.key === CONSENT_KEY || event.key === null) adopt(read());
		});
		const Channel = (win as unknown as { BroadcastChannel?: typeof BroadcastChannel })
			.BroadcastChannel;
		if (typeof Channel === 'function') {
			channel = new Channel(CONSENT_CHANNEL);
			channel.onmessage = (event: MessageEvent) => {
				try {
					adopt(read());
				} catch {
					const incoming = parse(JSON.stringify(event.data));
					if ((incoming.updatedAt ?? '') > (current?.updatedAt ?? '')) adopt(incoming);
				}
			};
		}
	};

	const get = (): ConsentRecord => {
		if (!current) current = read();
		return current;
	};

	const write = (record: ConsentRecord): boolean => {
		try {
			win.localStorage.setItem(CONSENT_KEY, JSON.stringify(record));
			return sameState(readStored(), record);
		} catch {
			return false;
		}
	};

	const reconcile = (intended: ConsentRecord, patch: Patch) => {
		const stored = readStored();
		const winner = usageWinner(stored, intended);
		const merged: ConsentRecord = {
			...winner,
			analytics: patch.analytics !== undefined ? patch.analytics : stored.analytics,
		};
		if (!sameState(stored, merged) && write(merged)) {
			failed = false;
			hardOff = false;
		}
		if (!sameState(current ?? empty(), merged)) adopt(merged);
	};

	return {
		get,
		read,
		saveFailed: () => failed,
		set(patch) {
			if (patch.usage === 'denied') hardOff = true;
			const before = read();
			let next = merge(before, patch);
			for (let i = 0; i < 3; i += 1) {
				const rawBase = readStored();
				next = merge(read(), patch);
				if (sameState(rawBase, readStored())) break;
			}
			if (write(next)) {
				failed = false;
				hardOff = false;
			} else {
				failed = true;
				if (patch.usage !== undefined && next.usage !== 'granted') {
					try {
						win.localStorage.removeItem(CONSENT_KEY);
					} catch {
						// this window stays off through hardOff
					}
				}
				if (patch.usage === 'granted') {
					next = { ...next, usage: before.usage, usageGeneration: before.usageGeneration };
				}
			}
			wire();
			channel?.postMessage(next);
			adopt(next);
			const locks = (win as unknown as { navigator?: { locks?: { request: (n: string, cb: () => void) => Promise<unknown> } } })
				.navigator?.locks;
			void locks?.request('tmgr-consent', () => reconcile(next, patch)).catch(() => {});
			return next;
		},
		subscribe(listener) {
			wire();
			listeners.add(listener);
			return () => listeners.delete(listener);
		},
	};
};

let shared: ConsentStore | null = null;

export const consentStore = (): ConsentStore => (shared ??= createConsentStore());
