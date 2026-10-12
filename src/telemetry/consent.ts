import { CONSENT_KEY } from '@/privacy/consent';
import { withLock } from './locks';

export const USAGE_KEY = 'tmgr:telemetry:consent';
export const TELEMETRY_CHANNEL = 'tmgr-telemetry';

export interface UsageView {
	on: boolean;
	generation: number;
	answered: boolean;
}

export interface UsageConsent {
	read(): UsageView;
	set(on: boolean): Promise<UsageView>;
	subscribe(listener: (change?: 'deny' | 'withdraw') => void | Promise<void>): () => void;
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

const migrateLegacy = (win: Window) => {
	try {
		const storage = win.localStorage;
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
	} catch {}
};

export const createUsageConsent = (win: Window = window): UsageConsent => {
	const listeners = new Set<(change?: 'deny' | 'withdraw') => void | Promise<void>>();
	const locks = win.navigator?.locks;
	let hardOff = false;
	let denied = false;
	let deniedThrough = 0;
	let generationHere = 0;
	let choice = 0;
	let answeredHere = false;
	let failed = false;
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
		generationHere = Math.max(s?.generation ?? 0, generationHere);
		return {
			on: !!s?.on && !hardOff && !denied,
			generation: generationHere,
			answered: s !== null || answeredHere,
		};
	};

	const notify = (change?: 'deny' | 'withdraw') =>
		Promise.all([...listeners].map((fn) => fn(change))).then(() => {});

	win.addEventListener('storage', (event: StorageEvent) => {
		if (event.key !== USAGE_KEY && event.key !== null) return;
		const incoming = parseStored(event.newValue);
		const current = stored();
		if (
			incoming?.on && current?.on &&
			incoming.generation === current.generation && current.generation > deniedThrough
		) denied = false;
		generationHere = Math.max(current?.generation ?? 0, generationHere);
		void notify();
	});
	read();
	const Channel = (win as unknown as { BroadcastChannel?: typeof BroadcastChannel }).BroadcastChannel;
	if (typeof Channel === 'function') {
		channel = new Channel(TELEMETRY_CHANNEL);
		channel.onmessage = (event: MessageEvent) => {
			if (event.data?.type !== 'deny') return;
			denied = true;
			deniedThrough = generationHere;
			answeredHere = true;
			void notify('deny');
		};
	}

	void withLock(locks, 'consent', () => migrateLegacy(win)).then(() => notify(), () => {});

	return {
		read,
		saveFailed: () => failed,
		async set(on) {
			const request = ++choice;
			if (!on) {
				hardOff = true;
				answeredHere = true;
				const generation = read().generation + 1;
				generationHere = generation;
				try {
					win.localStorage.setItem(USAGE_KEY, JSON.stringify({ on: false, generation }));
				} catch {
					try {
						win.localStorage.removeItem(USAGE_KEY);
					} catch {}
				}
				void notify('deny');
			}
			try {
				await withLock(locks, 'consent', async () => {
					const current = stored();
					const generation =
						!on && current && !current.on && current.generation === generationHere
							? generationHere
							: read().generation + 1;
					if (on && ((stored()?.generation ?? 0) !== generation - 1 || request !== choice)) return;
					generationHere = generation;
					let saved = false;
					try {
						win.localStorage.setItem(USAGE_KEY, JSON.stringify({ on, generation }));
						saved = true;
					} catch {
						if (!on) {
							try {
								win.localStorage.removeItem(USAGE_KEY);
							} catch {}
						}
					}
					failed = !saved;
					if (on && saved && request === choice) {
						hardOff = false;
						denied = false;
					}
					if (!on) {
						try {
							channel?.postMessage({ type: 'deny', generation });
						} catch {}
						await withLock(locks, 'send', () => notify('withdraw'));
					} else await notify();
				});
			} catch {
				failed = true;
				denied = true;
				await notify('deny');
			}
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
