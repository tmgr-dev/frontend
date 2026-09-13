import { RequestCache, requestCache } from '../requestCache';

describe('requestCache.getOrFetch (single-flight coalescing)', () => {
	beforeEach(() => {
		requestCache.clear();
	});

	afterEach(() => {
		requestCache.clear();
	});

	it('fetches once on a miss, caches the value, and serves it on the next call', async () => {
		const fetchFn = jest.fn().mockResolvedValue(7);

		const first = await requestCache.getOrFetch('k', fetchFn, { ttl: 10_000 });
		const second = await requestCache.getOrFetch('k', fetchFn, { ttl: 10_000 });

		expect(first).toBe(7);
		expect(second).toBe(7);
		expect(fetchFn).toHaveBeenCalledTimes(1);
	});

	it('coalesces concurrent identical requests into a single in-flight fetch', async () => {
		let resolveFetch!: (value: number) => void;
		const fetchFn = jest.fn(
			() => new Promise<number>((resolve) => (resolveFetch = resolve)),
		);

		const p1 = requestCache.getOrFetch('k', fetchFn, { ttl: 1_000 });
		const p2 = requestCache.getOrFetch('k', fetchFn, { ttl: 1_000 });

		// The root-cause fix: a second caller during the in-flight window must
		// NOT trigger a second network request.
		expect(fetchFn).toHaveBeenCalledTimes(1);

		resolveFetch(42);

		await expect(p1).resolves.toBe(42);
		await expect(p2).resolves.toBe(42);
	});

	it('refetches after the cached entry expires', async () => {
		jest.useFakeTimers();
		jest.setSystemTime(0);
		const fetchFn = jest
			.fn()
			.mockResolvedValueOnce('a')
			.mockResolvedValueOnce('b');

		expect(await requestCache.getOrFetch('k', fetchFn, { ttl: 1_000 })).toBe(
			'a',
		);

		jest.setSystemTime(2_000);

		expect(await requestCache.getOrFetch('k', fetchFn, { ttl: 1_000 })).toBe(
			'b',
		);
		expect(fetchFn).toHaveBeenCalledTimes(2);
		jest.useRealTimers();
	});

	it('caches null results and serves them without refetching', async () => {
		const fetchFn = jest.fn().mockResolvedValue(null);

		expect(
			await requestCache.getOrFetch('k', fetchFn, { ttl: 1_000 }),
		).toBeNull();
		expect(
			await requestCache.getOrFetch('k', fetchFn, { ttl: 1_000 }),
		).toBeNull();
		expect(fetchFn).toHaveBeenCalledTimes(1);
	});

	it('does not persist the resolved value when cache is disabled', async () => {
		const value = await requestCache.getOrFetch(
			'k',
			() => Promise.resolve('x'),
			{ cache: false },
		);

		expect(value).toBe('x');
		expect(requestCache.has('k')).toBe(false);
	});

	it('still coalesces concurrent calls when cache is disabled', async () => {
		let resolveFetch!: (value: string) => void;
		const fetchFn = jest.fn(
			() => new Promise<string>((resolve) => (resolveFetch = resolve)),
		);

		const p1 = requestCache.getOrFetch('k', fetchFn, { cache: false });
		const p2 = requestCache.getOrFetch('k', fetchFn, { cache: false });

		expect(fetchFn).toHaveBeenCalledTimes(1);

		resolveFetch('done');
		await Promise.all([p1, p2]);
	});

	it('clears the in-flight entry on rejection so retries refetch and errors are not cached', async () => {
		const fetchFn = jest
			.fn()
			.mockRejectedValueOnce(new Error('boom'))
			.mockResolvedValueOnce('ok');

		await expect(
			requestCache.getOrFetch('k', fetchFn, { ttl: 1_000 }),
		).rejects.toThrow('boom');

		expect(requestCache.has('k')).toBe(false);

		await expect(
			requestCache.getOrFetch('k', fetchFn, { ttl: 1_000 }),
		).resolves.toBe('ok');
		expect(fetchFn).toHaveBeenCalledTimes(2);
	});

	it('clears the in-flight entry even when fetchFn throws synchronously, so retries refetch (no poison entry)', async () => {
		const fetchFn = jest
			.fn()
			.mockImplementationOnce(() => {
				throw new Error('sync boom');
			})
			.mockResolvedValueOnce('ok');

		await expect(
			requestCache.getOrFetch('k', fetchFn, { ttl: 1_000 }),
		).rejects.toThrow('sync boom');

		expect(requestCache.has('k')).toBe(false);

		await expect(
			requestCache.getOrFetch('k', fetchFn, { ttl: 1_000 }),
		).resolves.toBe('ok');
		expect(fetchFn).toHaveBeenCalledTimes(2);
	});

	it('clearInFlight() drops pending requests so a new call starts a fresh fetch', async () => {
		let resolveFirst!: (value: number) => void;
		const first = jest.fn(
			() => new Promise<number>((resolve) => (resolveFirst = resolve)),
		);

		const p1 = requestCache.getOrFetch('k', first, { cache: false });
		requestCache.clearInFlight();

		const second = jest.fn().mockResolvedValue(2);
		const p2 = requestCache.getOrFetch('k', second, { cache: false });

		expect(second).toHaveBeenCalledTimes(1);

		resolveFirst(1);
		expect(await p1).toBe(1);
		expect(await p2).toBe(2);
	});

	it('clear() empties both the resolved cache and the in-flight map', async () => {
		await requestCache.getOrFetch('k', () => Promise.resolve(1), {
			ttl: 10_000,
		});
		expect(requestCache.has('k')).toBe(true);

		requestCache.clear();
		expect(requestCache.has('k')).toBe(false);
	});
});

function deferred<T>() {
	let resolve!: (value: T) => void;
	let reject!: (reason: Error) => void;
	const promise = new Promise<T>((res, rej) => {
		resolve = res;
		reject = rej;
	});
	return { promise, resolve, reject };
}

describe('requestCache invalidation lifecycle', () => {
	afterEach(() => requestCache.clear());

	it.each(['clear', 'clearInFlight', 'invalidate'] as const)(
		'%s prevents old responses from repopulating the cache',
		async (operation) => {
			const old = deferred<string>();
			const pending = requestCache.getOrFetch('tasks', () => old.promise);
			if (operation === 'invalidate') requestCache.invalidate('tasks');
			else requestCache[operation]();
			old.resolve('old');
			await expect(pending).resolves.toBe('old');
			expect(requestCache.has('tasks')).toBe(false);
		},
	);

	it.each(['resolve', 'reject'] as const)(
		'old %s does not remove the replacement single-flight request',
		async (settle) => {
			const old = deferred<string>();
			const fresh = deferred<string>();
			const first = requestCache.getOrFetch('tasks', () => old.promise);
			const settled = first.catch(() => undefined);
			requestCache.clear();
			const second = requestCache.getOrFetch('tasks', () => fresh.promise);
			if (settle === 'resolve') old.resolve('old');
			else old.reject(new Error('old failure'));
			await settled;
			const duplicate = jest.fn().mockResolvedValue('duplicate');
			const third = requestCache.getOrFetch('tasks', duplicate);
			expect(duplicate).not.toHaveBeenCalled();
			fresh.resolve('fresh');
			await expect(second).resolves.toBe('fresh');
			await expect(third).resolves.toBe('fresh');
		},
	);

	it('an old response cannot overwrite a completed replacement', async () => {
		const old = deferred<string>();
		const first = requestCache.getOrFetch('tasks', () => old.promise);
		requestCache.clear();
		await requestCache.getOrFetch('tasks', async () => 'fresh');
		old.resolve('old');
		await first;
		expect(requestCache.get('tasks')).toBe('fresh');
	});

	it('pattern invalidation removes matching pending requests without affecting unrelated ones', async () => {
		const a = deferred<string>();
		const b = deferred<string>();
		const other = deferred<string>();
		requestCache.set('tasks:cached', 'cached');
		const pending = [
			requestCache.getOrFetch('tasks:a', () => a.promise),
			requestCache.getOrFetch('tasks:b', () => b.promise),
			requestCache.getOrFetch('settings', () => other.promise),
		];
		requestCache.invalidate(/^tasks:/g);
		const replacement = jest.fn().mockResolvedValue('fresh');
		const replaced = requestCache.getOrFetch('tasks:a', replacement);
		expect(replacement).toHaveBeenCalledTimes(1);
		a.resolve('old a');
		b.resolve('old b');
		other.resolve('settings');
		await Promise.all([...pending, replaced]);
		expect(requestCache.get('tasks:a')).toBe('fresh');
		expect(requestCache.has('tasks:b')).toBe(false);
		expect(requestCache.has('tasks:cached')).toBe(false);
		expect(requestCache.get('settings')).toBe('settings');
	});
});

describe('requestCache bounded storage', () => {
	afterEach(() => {
		requestCache.clear();
		jest.useRealTimers();
	});

	it('bounds the default cache to 200 entries and evicts the least recently read', () => {
		for (let i = 0; i < 200; i++) requestCache.set(`key:${i}`, i);
		expect(requestCache.get('key:0')).toBe(0);
		requestCache.set('key:200', 200);
		expect(requestCache.size()).toBe(200);
		expect(requestCache.get('key:0')).toBe(0);
		expect(requestCache.has('key:1')).toBe(false);
		expect(requestCache.get('key:200')).toBe(200);
	});

	it('sweeps expired entries on writes before evicting live entries', () => {
		jest.useFakeTimers();
		jest.setSystemTime(0);
		requestCache.set('live', 'live', 10_000);
		for (let i = 0; i < 199; i++) requestCache.set(`expired:${i}`, i, 10);
		jest.setSystemTime(11);
		requestCache.set('new', 'new');
		expect(requestCache.size()).toBe(2);
		expect(requestCache.get('live')).toBe('live');
	});

	it('refreshing an existing entry makes it recently used without evicting another entry', () => {
		for (let i = 0; i < 200; i++) requestCache.set(`key:${i}`, i);
		requestCache.set('key:0', 'updated');
		expect(requestCache.size()).toBe(200);
		requestCache.set('new', 'new');
		expect(requestCache.get('key:0')).toBe('updated');
		expect(requestCache.has('key:1')).toBe(false);
	});
});

describe('RequestCache configuration', () => {
	it('honors a custom entry limit while retaining the TTL constructor argument', () => {
		jest.useFakeTimers();
		jest.setSystemTime(0);
		try {
			const cache = new RequestCache(50, 2);
			cache.set('a', 1);
			cache.set('b', 2);
			cache.set('c', 3);
			expect(cache.size()).toBe(2);
			expect(cache.has('a')).toBe(false);
			jest.setSystemTime(51);
			expect(cache.has('b')).toBe(false);
		} finally {
			jest.useRealTimers();
		}
	});

	it.each([0, -1, 1.5, Infinity, NaN])(
		'rejects invalid entry limit %s',
		(limit) => {
			expect(() => new RequestCache(1000, limit)).toThrow(RangeError);
		},
	);
});

it('namespaces the same logical key by session/workspace and retains logical invalidation', async () => {
	const cache = new RequestCache();
	cache.setContext('user-1:workspace-A');
	let finish!: (value: string) => void;
	const old = cache.getOrFetch(
		'categories',
		() =>
			new Promise<string>((resolve) => {
				finish = resolve;
			}),
	);
	cache.setContext('user-1:workspace-B');
	await cache.getOrFetch('categories', async () => 'B');
	finish('A');
	await old;
	expect(cache.get('categories')).toBe('B');
	cache.invalidate(/^categories/);
	expect(cache.get('categories')).toBeNull();
});
