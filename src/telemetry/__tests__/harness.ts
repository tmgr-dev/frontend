import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { USAGE_KEY } from '../consent';
import { createTelemetry, type TelemetryDeps } from '../client';

export const BASE = 'https://api.example.test/api/';

export class FakeChannel {
	static all: FakeChannel[] = [];
	onmessage: ((event: unknown) => void) | null = null;
	constructor(public name = 'tmgr-telemetry') {
		FakeChannel.all.push(this);
	}
	postMessage(data: unknown) {
		FakeChannel.all
			.filter((c) => c !== this && c.name === this.name)
			.forEach((c) => c.onmessage?.({ data }));
	}
}

export interface FetchCall {
	url: string;
	init: RequestInit & { headers: Record<string, string> };
}

export const setConsent = (usage: 'granted' | 'denied' | null, generation: number) =>
	usage === null
		? localStorage.removeItem(USAGE_KEY)
		: localStorage.setItem(USAGE_KEY, JSON.stringify({ on: usage === 'granted', generation }));

export const receiptKeys = (storage: Storage = localStorage): string[] =>
	Object.keys(storage).filter((key) => key.startsWith('tmgr:telemetry:receipt:'));

export const makeClock = (start = Date.UTC(2026, 9, 12, 10, 37, 21)) => {
	let time = start;
	return { now: () => time, advance: (ms: number) => (time += ms) };
};

export interface Tab {
	telemetry: ReturnType<typeof createTelemetry>;
	calls: FetchCall[];
	respond: { status: number | 'network'; delay?: Promise<void> };
	timers: { fn: () => void; ms: number; id: number; cleared: boolean }[];
	later: { fn: () => void; ms: number }[];
	clock: ReturnType<typeof makeClock>;
}

export const makeTab = (overrides: Partial<TelemetryDeps> = {}, clock = makeClock()): Tab => {
	const calls: FetchCall[] = [];
	const timers: Tab['timers'] = [];
	const later: Tab['later'] = [];
	const respond: Tab['respond'] = { status: 204 };
	const fetchFn = (async (url: string, init: FetchCall['init']) => {
		calls.push({ url, init });
		if (respond.delay) await respond.delay;
		if (init.signal?.aborted) throw new DOMException('aborted', 'AbortError');
		if (respond.status === 'network') throw new TypeError('network');
		return { status: respond.status } as Response;
	}) as unknown as typeof fetch;
	const telemetry = createTelemetry({
		storage: localStorage,
		now: clock.now,
		fetch: fetchFn,
		baseUrl: BASE,
		consent: {
			read: () => {
				const raw = JSON.parse(localStorage.getItem(USAGE_KEY) ?? '{}');
				return { on: raw.on === true, generation: raw.generation ?? 0 };
			},
			deny: () => {
				const raw = JSON.parse(localStorage.getItem(USAGE_KEY) ?? '{}');
				setConsent('denied', (raw.generation ?? 0) + 1);
			},
		},
		meta: () => ({
			platform: 'web',
			app_version: '3.4',
			account_mode: 'cloud',
			viewport: 'l',
		}),
		uuid: () => randomUUID(),
		secret: () => randomBytes(32).toString('hex'),
		sha256: async (text) => createHash('sha256').update(text).digest('hex'),
		setInterval: (fn, ms) => {
			timers.push({ fn, ms, id: timers.length, cleared: false });
			return timers.length - 1;
		},
		clearInterval: (id) => {
			timers[id as number].cleared = true;
		},
		later: (fn, ms) => later.push({ fn, ms }),
		...overrides,
	});
	return { telemetry, calls, respond, timers, later, clock };
};

export const tick = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

export class FlakyStorage {
	data = new Map<string, string>();
	failSet: ((key: string) => boolean) | null = null;
	failRemove = false;
	get length() {
		return this.data.size;
	}
	key(i: number) {
		return [...this.data.keys()][i] ?? null;
	}
	clear() {
		this.data.clear();
	}
	getItem(key: string) {
		return this.data.get(key) ?? null;
	}
	setItem(key: string, value: string) {
		if (this.failSet?.(key)) throw new Error('quota');
		this.data.set(key, value);
	}
	removeItem(key: string) {
		if (this.failRemove) throw new Error('blocked');
		this.data.delete(key);
	}
}
