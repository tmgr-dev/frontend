import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { CONSENT_KEY } from '@/privacy/consent';
import { createTelemetry, type ChannelLike, type TelemetryDeps } from '../client';

export const BASE = 'https://api.example.test/api/';

export class FakeChannel implements ChannelLike {
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

export const setConsent = (usage: 'granted' | 'denied' | null, usageGeneration: number) =>
	localStorage.setItem(
		CONSENT_KEY,
		JSON.stringify({ analytics: null, usage, usageGeneration, updatedAt: null }),
	);

export const makeClock = (start = Date.UTC(2026, 9, 12, 10, 37, 21)) => {
	let time = start;
	return { now: () => time, advance: (ms: number) => (time += ms) };
};

export interface Tab {
	telemetry: ReturnType<typeof createTelemetry>;
	calls: FetchCall[];
	respond: { status: number | 'network'; delay?: Promise<void> };
	timers: { fn: () => void; ms: number; id: number; cleared: boolean }[];
	clock: ReturnType<typeof makeClock>;
}

export const makeTab = (overrides: Partial<TelemetryDeps> = {}, clock = makeClock()): Tab => {
	const calls: FetchCall[] = [];
	const timers: Tab['timers'] = [];
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
				const raw = JSON.parse(localStorage.getItem(CONSENT_KEY) ?? '{}');
				return { on: raw.usage === 'granted', generation: raw.usageGeneration ?? 0 };
			},
			deny: () => {
				const raw = JSON.parse(localStorage.getItem(CONSENT_KEY) ?? '{}');
				setConsent('denied', (raw.usageGeneration ?? 0) + 1);
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
		channel: new FakeChannel(),
		...overrides,
	});
	return { telemetry, calls, respond, timers, clock };
};

export const tick = () => new Promise<void>((resolve) => setTimeout(resolve, 0));
