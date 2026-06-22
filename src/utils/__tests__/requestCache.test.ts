import { requestCache } from '../requestCache';

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
