export type ConsentValue = 'granted' | 'denied' | null;

export interface ConsentRecord {
	analytics: ConsentValue;
	usage: ConsentValue;
	updatedAt: string | null;
}

export type ConsentListener = (record: ConsentRecord) => void;

export const CONSENT_KEY = 'tmgr:consent:v1';
export const CONSENT_CHANNEL = 'tmgr-consent';

const empty = (): ConsentRecord => ({
	analytics: null,
	usage: null,
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
			updatedAt: typeof data?.updatedAt === 'string' ? data.updatedAt : null,
		};
	} catch {
		return empty();
	}
};

export interface ConsentStore {
	get(): ConsentRecord;
	set(patch: Partial<Pick<ConsentRecord, 'analytics' | 'usage'>>): ConsentRecord;
	subscribe(listener: ConsentListener): () => void;
}

export const createConsentStore = (win: Window = window): ConsentStore => {
	const listeners = new Set<ConsentListener>();
	let current: ConsentRecord | null = null;
	let channel: BroadcastChannel | null = null;
	let wired = false;

	const read = (): ConsentRecord => {
		try {
			return parse(win.localStorage.getItem(CONSENT_KEY));
		} catch {
			return current ?? empty();
		}
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
					adopt(parse(win.localStorage.getItem(CONSENT_KEY)));
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

	return {
		get,
		set(patch) {
			const next: ConsentRecord = {
				...get(),
				...(patch.analytics !== undefined && { analytics: patch.analytics }),
				...(patch.usage !== undefined && { usage: patch.usage }),
				updatedAt: new Date().toISOString(),
			};
			try {
				win.localStorage.setItem(CONSENT_KEY, JSON.stringify(next));
			} catch {
				// kept in memory for this tab
			}
			wire();
			channel?.postMessage(next);
			adopt(next);
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
