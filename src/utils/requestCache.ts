interface CacheEntry<T> {
	data: T;
	timestamp: number;
	expiresAt: number;
}

class RequestCache {
	private cache: Map<string, CacheEntry<any>>;
	private defaultTTL: number;

	constructor(defaultTTL: number = 5 * 60 * 1000) {
		this.cache = new Map();
		this.defaultTTL = defaultTTL;
	}

	generateKey(url: string, params?: any): string {
		return `${url}${params ? JSON.stringify(params) : ''}`;
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
			keysToDelete.forEach(key => this.cache.delete(key));
		}
	}

	clear(): void {
		this.cache.clear();
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
