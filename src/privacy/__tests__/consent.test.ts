/** @jest-environment jsdom */
import { CONSENT_CHANNEL, CONSENT_KEY, createConsentStore } from '../consent';

class FakeChannel {
	static all: FakeChannel[] = [];
	onmessage: ((event: { data: unknown }) => void) | null = null;
	constructor(public name: string) {
		FakeChannel.all.push(this);
	}
	postMessage(data: unknown) {
		FakeChannel.all
			.filter((c) => c !== this && c.name === this.name)
			.forEach((c) => c.onmessage?.({ data }));
	}
}

const freshWindow = () => {
	const win = document.defaultView as unknown as Record<string, unknown>;
	win.BroadcastChannel = FakeChannel;
	return win as unknown as Window;
};

beforeEach(() => {
	localStorage.clear();
	FakeChannel.all = [];
});

describe('consent store', () => {
	it('defaults to no choice', () => {
		const store = createConsentStore(freshWindow());
		expect(store.get()).toEqual({ analytics: null, usage: null, updatedAt: null });
	});

	it('persists a choice and notifies subscribers', () => {
		const store = createConsentStore(freshWindow());
		const seen: unknown[] = [];
		store.subscribe((r) => seen.push(r.analytics));
		store.set({ analytics: 'granted' });
		expect(store.get().analytics).toBe('granted');
		expect(store.get().usage).toBeNull();
		expect(JSON.parse(localStorage.getItem(CONSENT_KEY)!).analytics).toBe('granted');
		expect(seen).toEqual(['granted']);
	});

	it('reads an existing record and ignores garbage values', () => {
		localStorage.setItem(
			CONSENT_KEY,
			JSON.stringify({ analytics: 'denied', usage: 'maybe', updatedAt: 5 }),
		);
		const store = createConsentStore(freshWindow());
		expect(store.get()).toEqual({ analytics: 'denied', usage: null, updatedAt: null });
	});

	it('survives corrupt JSON', () => {
		localStorage.setItem(CONSENT_KEY, '{nope');
		expect(createConsentStore(freshWindow()).get().analytics).toBeNull();
	});

	it('follows a change made in another tab via the storage event', () => {
		const win = freshWindow();
		const store = createConsentStore(win);
		const seen: unknown[] = [];
		store.subscribe((r) => seen.push(r.analytics));
		localStorage.setItem(CONSENT_KEY, JSON.stringify({ analytics: 'granted', usage: null }));
		win.dispatchEvent(new StorageEvent('storage', { key: CONSENT_KEY }));
		expect(store.get().analytics).toBe('granted');
		expect(seen).toEqual(['granted']);
	});

	it('follows a change announced on the BroadcastChannel', () => {
		const a = createConsentStore(freshWindow());
		const b = createConsentStore(freshWindow());
		const seen: unknown[] = [];
		b.subscribe((r) => seen.push(r.analytics));
		a.subscribe(() => {});
		a.set({ analytics: 'denied' });
		expect(b.get().analytics).toBe('denied');
		expect(seen).toEqual(['denied']);
		expect(FakeChannel.all.every((c) => c.name === CONSENT_CHANNEL)).toBe(true);
	});

	it('keeps working in memory when localStorage throws', () => {
		const win = freshWindow();
		const spy = jest.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
			throw new Error('blocked');
		});
		const set = jest.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
			throw new Error('blocked');
		});
		const store = createConsentStore(win);
		expect(store.get().analytics).toBeNull();
		expect(() => store.set({ analytics: 'granted' })).not.toThrow();
		expect(store.get().analytics).toBe('granted');
		spy.mockRestore();
		set.mockRestore();
	});

	it('a delayed broadcast cannot override the stored choice', () => {
		const win = freshWindow();
		const a = createConsentStore(win);
		const b = createConsentStore(win);
		b.subscribe(() => {});
		a.subscribe(() => {});
		const stale = { analytics: 'granted', usage: null, updatedAt: '2026-01-01T00:00:00.000Z' };
		b.set({ analytics: 'denied' });
		FakeChannel.all[1].postMessage(stale);
		expect(a.get().analytics).toBe('denied');
		expect(b.get().analytics).toBe('denied');
	});
});
