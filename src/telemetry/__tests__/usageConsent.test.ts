/** @jest-environment jsdom */
import { CONSENT_KEY, createConsentStore } from '@/privacy/consent';
import { createUsageConsent, USAGE_KEY } from '../consent';
import { FakeChannel, FlakyStorage } from './harness';

class SpyChannel extends FakeChannel {
	posts: unknown[] = [];
	postMessage(data: unknown) {
		this.posts.push(data);
		super.postMessage(data);
	}
}

const winWith = (storage: unknown) => {
	const listeners: Record<string, Array<(e: unknown) => void>> = {};
	return {
		localStorage: storage,
		addEventListener: (type: string, fn: (e: unknown) => void) => (listeners[type] ??= []).push(fn),
		BroadcastChannel: SpyChannel,
		fire: (type: string, e: unknown) => listeners[type]?.forEach((fn) => fn(e)),
	} as unknown as Window & { fire: (type: string, e: unknown) => void };
};

const stored = (s: FlakyStorage, key = USAGE_KEY) => JSON.parse(s.data.get(key) ?? 'null');

beforeEach(() => {
	FakeChannel.all = [];
});

describe('separate keys', () => {
	it('analytics writes never touch the usage key and the reverse', () => {
		const s = new FlakyStorage();
		const win = winWith(s);
		const usage = createUsageConsent(win);
		const analytics = createConsentStore(win);
		usage.set(true);
		const before = s.data.get(USAGE_KEY);
		analytics.set({ analytics: 'granted' });
		expect(s.data.get(USAGE_KEY)).toBe(before);
		const analyticsBefore = s.data.get(CONSENT_KEY);
		usage.set(false);
		expect(s.data.get(CONSENT_KEY)).toBe(analyticsBefore);
		expect(JSON.parse(analyticsBefore!)).not.toHaveProperty('usage');
		expect(JSON.parse(analyticsBefore!)).not.toHaveProperty('usageGeneration');
	});

	it('migrates an old usage answer once and strips it from the analytics record', () => {
		const s = new FlakyStorage();
		s.data.set(
			CONSENT_KEY,
			JSON.stringify({ analytics: 'denied', usage: 'granted', usageGeneration: 3, updatedAt: 'x' }),
		);
		const usage = createUsageConsent(winWith(s));
		expect(stored(s)).toEqual({ on: true, generation: 3 });
		expect(JSON.parse(s.data.get(CONSENT_KEY)!)).toEqual({ analytics: 'denied', updatedAt: 'x' });
		expect(usage.read()).toMatchObject({ on: true, generation: 3, answered: true });
	});

	it('does not overwrite an existing usage record when migrating', () => {
		const s = new FlakyStorage();
		s.data.set(USAGE_KEY, JSON.stringify({ on: false, generation: 9 }));
		s.data.set(CONSENT_KEY, JSON.stringify({ analytics: null, usage: 'granted', usageGeneration: 3 }));
		createUsageConsent(winWith(s));
		expect(stored(s)).toEqual({ on: false, generation: 9 });
	});

	it('is unanswered until the user chooses', () => {
		const usage = createUsageConsent(winWith(new FlakyStorage()));
		expect(usage.read()).toEqual({ on: false, generation: 0, answered: false });
	});
});

describe('toggling', () => {
	it('writes the stored generation plus one, last click wins', () => {
		const s = new FlakyStorage();
		const usage = createUsageConsent(winWith(s));
		usage.set(true);
		expect(stored(s)).toEqual({ on: true, generation: 1 });
		usage.set(false);
		expect(stored(s)).toEqual({ on: false, generation: 2 });
		s.data.set(USAGE_KEY, JSON.stringify({ on: true, generation: 7 }));
		usage.set(false);
		expect(stored(s)).toEqual({ on: false, generation: 8 });
	});

	it('broadcasts a deny with its generation and nothing on a grant', () => {
		const win = winWith(new FlakyStorage());
		const usage = createUsageConsent(win);
		const channel = FakeChannel.all[FakeChannel.all.length - 1] as SpyChannel;
		usage.set(true);
		expect(channel.posts).toEqual([]);
		usage.set(false);
		expect(channel.posts).toEqual([{ type: 'deny', generation: 2 }]);
	});
});

describe('asymmetric signals', () => {
	const pair = () => {
		const s = new FlakyStorage();
		const a = createUsageConsent(winWith(s));
		a.set(true);
		const bWin = winWith(s);
		const b = createUsageConsent(bWin);
		b.subscribe(() => {});
		return { s, a, b, bWin };
	};

	it('a received deny stops the tab without reading storage', () => {
		const { s, b } = pair();
		expect(b.read().on).toBe(true);
		const other = new SpyChannel();
		other.postMessage({ type: 'deny', generation: 2 });
		expect(stored(s)).toEqual({ on: true, generation: 1 });
		expect(b.read().on).toBe(false);
	});

	it('adopts a stale deny and adopts a later grant only from storage', () => {
		const { s, b, bWin } = pair();
		new SpyChannel().postMessage({ type: 'deny', generation: 0 });
		expect(b.read().on).toBe(false);
		new SpyChannel().postMessage({ type: 'deny', generation: 2 });
		expect(b.read().on).toBe(false);
		new SpyChannel().postMessage({ type: 'grant', generation: 3 });
		expect(b.read().on).toBe(false);
		s.data.set(USAGE_KEY, JSON.stringify({ on: true, generation: 3 }));
		bWin.fire('storage', { key: USAGE_KEY, newValue: s.data.get(USAGE_KEY) });
		expect(b.read().on).toBe(true);
	});

	it('twenty delivered denies cause no rebroadcast', () => {
		const { b } = pair();
		const mine = FakeChannel.all[FakeChannel.all.length - 1] as SpyChannel;
		const other = new SpyChannel();
		for (let i = 0; i < 20; i += 1) other.postMessage({ type: 'deny', generation: 2 + i });
		expect(mine.posts).toEqual([]);
		expect(b.read().on).toBe(false);
	});

	it('a failed local denial stays off until this tab grants again', () => {
		const s = new FlakyStorage();
		const win = winWith(s);
		const usage = createUsageConsent(win);
		usage.set(true);
		s.failSet = () => true;
		s.failRemove = true;
		usage.set(false);
		expect(stored(s)).toEqual({ on: true, generation: 1 });
		expect(usage.read().on).toBe(false);
		expect(usage.saveFailed()).toBe(true);
		s.data.set(USAGE_KEY, JSON.stringify({ on: true, generation: 50 }));
		win.fire('storage', { key: USAGE_KEY });
		new SpyChannel().postMessage({ type: 'grant', generation: 51 });
		expect(usage.read().on).toBe(false);
		s.failSet = null;
		usage.set(true);
		expect(usage.read().on).toBe(true);
		expect(usage.saveFailed()).toBe(false);
	});

	it('removes the stored grant when the denial write fails but removal works', () => {
		const s = new FlakyStorage();
		const usage = createUsageConsent(winWith(s));
		usage.set(true);
		s.failSet = () => true;
		usage.set(false);
		expect(s.data.has(USAGE_KEY)).toBe(false);
		expect(usage.read().on).toBe(false);
		expect(usage.read().answered).toBe(true);
	});

	it('notifies subscribers on a storage event', () => {
		const s = new FlakyStorage();
		const win = winWith(s);
		const usage = createUsageConsent(win);
		const seen: boolean[] = [];
		usage.subscribe(() => { seen.push(usage.read().on); });
		s.data.set(USAGE_KEY, JSON.stringify({ on: true, generation: 1 }));
		win.fire('storage', { key: USAGE_KEY });
		expect(seen).toEqual([true]);
	});
});
