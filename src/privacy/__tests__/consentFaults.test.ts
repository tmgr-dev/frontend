/** @jest-environment jsdom */
import { CONSENT_KEY, createConsentStore } from '../consent';

class FlakyStorage {
	data = new Map<string, string>();
	failSet: ((key: string, value: string) => boolean) | null = null;
	failRemove = false;
	onGet: ((key: string, n: number) => void) | null = null;
	gets = 0;
	getItem(key: string) {
		this.gets += 1;
		this.onGet?.(key, this.gets);
		return this.data.get(key) ?? null;
	}
	setItem(key: string, value: string) {
		if (this.failSet?.(key, value)) throw new Error('quota');
		this.data.set(key, value);
	}
	removeItem(key: string) {
		if (this.failRemove) throw new Error('blocked');
		this.data.delete(key);
	}
}

const winWith = (storage: FlakyStorage, locks?: unknown) =>
	({
		localStorage: storage,
		addEventListener: () => {},
		navigator: { locks },
	}) as unknown as Window;

const stored = (s: FlakyStorage) => JSON.parse(s.data.get(CONSENT_KEY) ?? 'null');

describe('a denial that cannot be saved', () => {
	const grant = (s: FlakyStorage) => {
		const store = createConsentStore(winWith(s));
		store.set({ usage: 'granted' });
		return store;
	};

	it('removes the stored grant when the write fails but removal works', () => {
		const s = new FlakyStorage();
		const store = grant(s);
		s.failSet = () => true;
		store.set({ usage: 'denied' });
		expect(s.data.has(CONSENT_KEY)).toBe(false);
		expect(store.read().usage).not.toBe('granted');
		expect(store.saveFailed()).toBe(true);
	});

	it('keeps this window hard off when nothing can be written or removed', () => {
		const s = new FlakyStorage();
		const store = grant(s);
		s.failSet = () => true;
		s.failRemove = true;
		store.set({ usage: 'denied' });
		expect(stored(s).usage).toBe('granted');
		expect(store.read().usage).toBe('denied');
		expect(store.get().usage).toBe('denied');
		expect(store.saveFailed()).toBe(true);
	});

	it('clears the error once a later write succeeds', () => {
		const s = new FlakyStorage();
		const store = grant(s);
		s.failSet = () => true;
		s.failRemove = true;
		store.set({ usage: 'denied' });
		s.failSet = null;
		store.set({ usage: 'denied' });
		expect(store.saveFailed()).toBe(false);
		expect(stored(s).usage).toBe('denied');
	});
});

describe('concurrent consent writes', () => {
	it('an analytics-only update never writes back a stale usage answer', () => {
		const s = new FlakyStorage();
		const store = createConsentStore(winWith(s));
		store.set({ usage: 'granted' });
		let injected = false;
		s.onGet = (key, n) => {
			if (injected || key !== CONSENT_KEY || n < 2) return;
			injected = true;
			s.data.set(
				CONSENT_KEY,
				JSON.stringify({ analytics: null, usage: 'denied', usageGeneration: 5, updatedAt: null }),
			);
		};
		store.set({ analytics: 'granted' });
		expect(stored(s)).toMatchObject({ analytics: 'granted', usage: 'denied', usageGeneration: 5 });
	});

	it('never writes a lower generation than the stored one', () => {
		const s = new FlakyStorage();
		s.data.set(
			CONSENT_KEY,
			JSON.stringify({ analytics: null, usage: 'denied', usageGeneration: 7, updatedAt: null }),
		);
		const store = createConsentStore(winWith(s));
		store.set({ analytics: 'denied' });
		expect(stored(s).usageGeneration).toBe(7);
		store.set({ usage: 'granted' });
		expect(stored(s)).toMatchObject({ usage: 'granted', usageGeneration: 8 });
	});

	it('re-checks inside the Web Lock and repairs a write that was lost', async () => {
		const s = new FlakyStorage();
		const pending: Array<() => unknown> = [];
		const locks = { request: (_name: string, cb: () => unknown) => Promise.resolve(pending.push(cb)) };
		const store = createConsentStore(winWith(s, locks));
		let lose = true;
		s.failSet = () => (lose ? ((lose = false), true) : false);
		s.failRemove = true;
		store.set({ usage: 'denied' });
		expect(s.data.has(CONSENT_KEY)).toBe(false);
		pending.forEach((cb) => cb());
		expect(stored(s)).toMatchObject({ usage: 'denied', usageGeneration: 1 });
		expect(store.saveFailed()).toBe(false);
	});

	it('inside the lock a higher stored generation wins over our older intent', () => {
		const s = new FlakyStorage();
		const pending: Array<() => unknown> = [];
		const locks = { request: (_n: string, cb: () => unknown) => Promise.resolve(pending.push(cb)) };
		const store = createConsentStore(winWith(s, locks));
		store.set({ usage: 'granted' });
		s.data.set(
			CONSENT_KEY,
			JSON.stringify({ analytics: null, usage: 'denied', usageGeneration: 4, updatedAt: null }),
		);
		pending.forEach((cb) => cb());
		expect(stored(s)).toMatchObject({ usage: 'denied', usageGeneration: 4 });
		expect(store.get().usage).toBe('denied');
	});
});
