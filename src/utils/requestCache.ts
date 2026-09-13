interface CacheEntry<T> {
	data: T;
	timestamp: number;
	expiresAt: number;
}

interface GetOrFetchOptions {
	ttl?: number;
	cache?: boolean;
}

export class RequestCache {
	private cache: Map<string, CacheEntry<any>>;
	private inFlight: Map<string, Promise<any>>;
	private defaultTTL: number;
	private maxEntries: number;
	private context = '';
	private scoped(key: string) {
		return this.context ? `${this.context}\0${key}` : key;
	}
	getContext() {
		return this.context;
	}
	setContext(context: string) {
		if (this.context === context) return;
		this.clear();
		this.context = context;
	}

	constructor(defaultTTL: number = 5 * 60 * 1000, maxEntries: number = 200) {
		if (!Number.isInteger(maxEntries) || maxEntries < 1) {
			throw new RangeError('maxEntries must be a positive integer');
		}
		this.cache = new Map();
		this.inFlight = new Map();
		this.defaultTTL = defaultTTL;
		this.maxEntries = maxEntries;
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
		const pendingKey = this.scoped(key);

		if (cache && this.has(key)) {
			return this.get<T>(key) as T;
		}

		const pending = this.inFlight.get(pendingKey);
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
				if (cache && this.inFlight.get(pendingKey) === request) {
					this.set(key, data, ttl);
				}
				return data;
			})
			.finally(() => {
				if (this.inFlight.get(pendingKey) === request) {
					this.inFlight.delete(pendingKey);
				}
			});

		this.inFlight.set(pendingKey, request);
		return request;
	}

	get<T>(key: string): T | null {
		key = this.scoped(key);
		const entry = this.cache.get(key);

		if (!entry) {
			return null;
		}

		if (Date.now() > entry.expiresAt) {
			this.cache.delete(key);
			return null;
		}

		// Map insertion order tracks least recently used entries.
		this.cache.delete(key);
		this.cache.set(key, entry);
		return entry.data;
	}

	set<T>(key: string, data: T, ttl?: number): void {
		key = this.scoped(key);
		const timestamp = Date.now();
		const expiresAt = timestamp + (ttl || this.defaultTTL);

		for (const [cachedKey, entry] of this.cache) {
			if (timestamp > entry.expiresAt) this.cache.delete(cachedKey);
		}
		this.cache.delete(key);
		this.cache.set(key, {
			data,
			timestamp,
			expiresAt,
		});
		while (this.cache.size > this.maxEntries) {
			this.cache.delete(this.cache.keys().next().value as string);
		}
	}

	invalidate(keyOrPattern: string | RegExp): void {
		if (typeof keyOrPattern === 'string') {
			keyOrPattern = this.scoped(keyOrPattern);
			this.cache.delete(keyOrPattern);
			this.inFlight.delete(keyOrPattern);
			return;
		}

		const keys = new Set([...this.cache.keys(), ...this.inFlight.keys()]);
		const originalLastIndex = keyOrPattern.lastIndex;
		for (const key of keys) {
			// Global/sticky patterns must match each key independently.
			keyOrPattern.lastIndex = 0;
			if (keyOrPattern.test(key.slice(key.indexOf('\0') + 1))) {
				this.cache.delete(key);
				this.inFlight.delete(key);
			}
		}
		keyOrPattern.lastIndex = originalLastIndex;
	}

	clearInFlight(): void {
		this.inFlight.clear();
	}

	clear(): void {
		this.cache.clear();
		this.inFlight.clear();
	}

	has(key: string): boolean {
		key = this.scoped(key);
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
