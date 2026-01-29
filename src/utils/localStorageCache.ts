interface CacheItem<T> {
	data: T;
	expiry: number;
}

export function getWithExpiry<T>(key: string): T | null {
	try {
		const item = localStorage.getItem(key);
		if (!item) return null;

		const cacheItem: CacheItem<T> = JSON.parse(item);
		
		if (Date.now() > cacheItem.expiry) {
			localStorage.removeItem(key);
			return null;
		}
		
		return cacheItem.data;
	} catch (error) {
		console.error(`Failed to get cached item "${key}":`, error);
		localStorage.removeItem(key);
		return null;
	}
}

export function setWithExpiry<T>(key: string, data: T, ttlMs: number): void {
	try {
		const cacheItem: CacheItem<T> = {
			data,
			expiry: Date.now() + ttlMs,
		};
		localStorage.setItem(key, JSON.stringify(cacheItem));
	} catch (error) {
		console.error(`Failed to set cached item "${key}":`, error);
	}
}

export function removeCache(key: string): void {
	try {
		localStorage.removeItem(key);
	} catch (error) {
		console.error(`Failed to remove cached item "${key}":`, error);
	}
}

export function clearExpiredCache(): void {
	try {
		const keys = Object.keys(localStorage);
		for (const key of keys) {
			const item = localStorage.getItem(key);
			if (item) {
				try {
					const parsed = JSON.parse(item);
					if (parsed.expiry && Date.now() > parsed.expiry) {
						localStorage.removeItem(key);
					}
				} catch {
				}
			}
		}
	} catch (error) {
		console.error('Failed to clear expired cache:', error);
	}
}

export const CACHE_KEYS = {
	WORKSPACES: 'cache_workspaces',
	WORKSPACE_STATUSES: 'cache_workspace_statuses',
	CATEGORIES: 'cache_categories',
	USER_SETTINGS: 'cache_user_settings',
} as const;

export const CACHE_TTL = {
	SHORT: 1 * 60 * 1000,
	MEDIUM: 5 * 60 * 1000,
	LONG: 15 * 60 * 1000,
	VERY_LONG: 60 * 60 * 1000,
} as const;
