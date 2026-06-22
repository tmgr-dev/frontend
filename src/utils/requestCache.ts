interface CacheEntry<T> {
	data: T;
	timestamp: number;
	expiresAt: number;
}

interface GetOrFetchOptions {
	ttl?: number;
	cache?: boolean;
}

class RequestCache {
	private cache: Map<string, CacheEntry<any>>;
	private inFlight: Map<string, Promise<any>>;
	private defaultTTL: number;

	constructor(defaultTTL: number = 5 * 60 * 1000) {
		this.cache = new Map();
		this.inFlight = new Map();
		this.defaultTTL = defaultTTL;
	}

	generateKey(url: string, params?: any): string {
		return `${url}${params ? JSON.stringify(params) : ''}`;
	}

	async getOrFetch<T>(
		key: string,
		fetchFn: () => Promise<T>,
		options: GetOrFetchOptions = {},
	): Promise<T> {
		const { ttl, cache = true } = options;

		if (cache && this.has(key)) {
			return this.get<T>(key) as T;
		}

		const pending = this.inFlight.get(key);
		if (pending) {
			return pending as Promise<T>;
		}

		let started: Promise<T>;
		try {
			started = Promise.resolve(fetchFn());
		} catch (error) {
			started = Promise.reject(error);
		}

		const request = started
			.then((data) => {
				if (cache) {
					this.set(key, data, ttl);
				}
				return data;
			})
			.finally(() => {
				this.inFlight.delete(key);
			});

		this.inFlight.set(key, request);
		return request;
	}

	get<T>(key: string): T | null {
		const entry = this.cache.get(key);

		if (!entry) {
			return null;
		}

		if (Date.now() > entry.expiresAt) {
			this.cache.delete(key);
			return null;
		}

		return entry.data;
	}

	set<T>(key: string, data: T, ttl?: number): void {
		const timestamp = Date.now();
		const expiresAt = timestamp + (ttl || this.defaultTTL);

		this.cache.set(key, {
			data,
			timestamp,
			expiresAt,
		});
	}

	invalidate(keyOrPattern: string | RegExp): void {
		if (typeof keyOrPattern === 'string') {
			this.cache.delete(keyOrPattern);
		} else {
			const keysToDelete: string[] = [];
			this.cache.forEach((_, key) => {
				if (keyOrPattern.test(key)) {
					keysToDelete.push(key);
				}
			});
			keysToDelete.forEach((key) => this.cache.delete(key));
		}
	}

	clearInFlight(): void {
		this.inFlight.clear();
	}

	clear(): void {
		this.cache.clear();
		this.inFlight.clear();
	}

	has(key: string): boolean {
		const entry = this.cache.get(key);
		if (!entry) return false;

		if (Date.now() > entry.expiresAt) {
			this.cache.delete(key);
			return false;
		}

		return true;
	}

	size(): number {
		return this.cache.size;
	}
}

export const requestCache = new RequestCache();

export default requestCache;
