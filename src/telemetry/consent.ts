import { CONSENT_KEY } from '@/privacy/consent';

export const USAGE_KEY = 'tmgr:telemetry:consent';
export const TELEMETRY_CHANNEL = 'tmgr-telemetry';

export interface UsageView {
	on: boolean;
	generation: number;
	answered: boolean;
}

export interface UsageConsent {
	read(): UsageView;
	set(on: boolean): UsageView;
	subscribe(listener: () => void): () => void;
	saveFailed(): boolean;
}

interface Stored {
	on: boolean;
	generation: number;
}

const parseStored = (raw: string | null): Stored | null => {
	if (!raw) return null;
	try {
		const data = JSON.parse(raw);
		return typeof data?.on === 'boolean' && Number.isInteger(data.generation) && data.generation >= 0
			? { on: data.on, generation: data.generation }
			: null;
	} catch {
		return null;
	}
};

const migrateLegacy = (storage: Storage) => {
	try {
		const raw = storage.getItem(CONSENT_KEY);
		if (!raw) return;
		const old = JSON.parse(raw);
		if (!old || typeof old !== 'object' || !('usage' in old || 'usageGeneration' in old)) return;
		if (storage.getItem(USAGE_KEY) === null && (old.usage === 'granted' || old.usage === 'denied')) {
			const generation = Number.isInteger(old.usageGeneration) ? Math.max(1, old.usageGeneration) : 1;
			storage.setItem(USAGE_KEY, JSON.stringify({ on: old.usage === 'granted', generation }));
		}
		const { usage: _usage, usageGeneration: _generation, ...rest } = old;
		storage.setItem(CONSENT_KEY, JSON.stringify(rest));
	} catch {
		// the legacy record is left as it is
	}
};

export const createUsageConsent = (win: Window = window): UsageConsent => {
	const listeners = new Set<() => void>();
	let hardOff = false;
	let floor = 0;
	let answeredHere = false;
	let failed = false;
	let wired = false;
	let channel: BroadcastChannel | null = null;

	const stored = (): Stored | null => {
		try {
			return parseStored(win.localStorage.getItem(USAGE_KEY));
		} catch {
			return null;
		}
	};

	const read = (): UsageView => {
		const s = stored();
		const generation = Math.max(s?.generation ?? 0, floor);
		return {
			on: !!s?.on && !hardOff && s.generation > floor,
			generation,
			answered: s !== null || answeredHere,
		};
	};

	const notify = () => listeners.forEach((fn) => fn());

	const wire = () => {
		if (wired) return;
		wired = true;
		win.addEventListener('storage', (event: StorageEvent) => {
			if (event.key === USAGE_KEY || event.key === null) notify();
		});
		const Channel = (win as unknown as { BroadcastChannel?: typeof BroadcastChannel }).BroadcastChannel;
		if (typeof Channel === 'function') {
			channel = new Channel(TELEMETRY_CHANNEL);
			channel.onmessage = (event: MessageEvent) => {
				const message = event.data;
				if (message?.type !== 'deny' || !Number.isInteger(message.generation)) return;
				if (message.generation < read().generation) return;
				floor = message.generation;
				notify();
			};
		}
	};

	migrateLegacy(win.localStorage);
	wire();

	return {
		read,
		saveFailed: () => failed,
		set(on) {
			const generation = read().generation + 1;
			if (on) hardOff = false;
			else {
				hardOff = true;
				floor = generation;
				answeredHere = true;
			}
			let saved = false;
			try {
				win.localStorage.setItem(USAGE_KEY, JSON.stringify({ on, generation }));
				saved = true;
			} catch {
				if (!on) {
					try {
						win.localStorage.removeItem(USAGE_KEY);
					} catch {
						// this window stays off through hardOff
					}
				}
			}
			failed = !saved;
			if (!on) channel?.postMessage({ type: 'deny', generation });
			notify();
			return read();
		},
		subscribe(listener) {
			listeners.add(listener);
			return () => listeners.delete(listener);
		},
	};
};

let shared: UsageConsent | null = null;

export const usageConsent = (): UsageConsent => (shared ??= createUsageConsent());
