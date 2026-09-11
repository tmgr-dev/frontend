import type { TokenEnvelope } from '@/utils/tokenRefresher';

export const TOKEN_STORAGE_KEY = 'token';

/** Envelope persisted by the store's setToken mutation; null when absent or unreadable. */
export const parseStoredToken = (raw: string | null): TokenEnvelope | null => {
	if (!raw) {
		return null;
	}
	try {
		const parsed = JSON.parse(raw);
		return parsed && typeof parsed === 'object' && typeof parsed.token === 'string'
			? (parsed as TokenEnvelope)
			: null;
	} catch {
		return null;
	}
};

export type TokenSyncResult =
	| { changed: false }
	| { changed: true; token: TokenEnvelope | null };

/**
 * Decide what a `storage` event from another tab means for this tab's
 * session: a rotated token to adopt, a logout to mirror, or nothing.
 */
export const tokenFromStorageEvent = (
	event: { key: string | null; newValue: string | null },
	current: TokenEnvelope | null | undefined,
): TokenSyncResult => {
	if (event.key !== TOKEN_STORAGE_KEY) {
		return { changed: false };
	}
	const next = parseStoredToken(event.newValue);
	const same =
		(next?.token ?? null) === (current?.token ?? null) &&
		(next?.refresh_token ?? null) === (current?.refresh_token ?? null);
	return same ? { changed: false } : { changed: true, token: next };
};
