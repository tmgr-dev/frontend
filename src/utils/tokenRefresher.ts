export interface TokenEnvelope {
	token: string;
	refresh_token?: string;
	[key: string]: unknown;
}

export interface TokenRefresherDeps {
	/** Envelope as persisted in storage shared by every tab (localStorage). */
	readStored: () => TokenEnvelope | null;
	/** Access token this tab is sending right now (its in-memory copy). */
	currentAccessToken: () => string | undefined;
	post: (refreshToken: string) => Promise<TokenEnvelope>;
	onSuccess: (envelope: TokenEnvelope) => void;
	/** Cross-tab mutual exclusion around one refresh; defaults to no lock. */
	withLock?: <T>(fn: () => Promise<T>) => Promise<T>;
}

const AUTH_URL_PATTERN = /^\/?auth\/(login|register|refresh)(\/|$)/;

export const isAuthUrl = (url: string | undefined): boolean =>
	!!url && AUTH_URL_PATTERN.test(url);

const noLock = <T>(fn: () => Promise<T>) => fn();

// Deliberately NOT requestCache.getOrFetch: logout() calls
// requestCache.clear(), which must never cancel an in-flight token refresh,
// and a failed refresh resolves null instead of rejecting.
//
// The refresh token is read from shared storage under a cross-tab lock, and
// re-read after the lock is acquired: when another tab already rotated the
// token this tab adopts that envelope instead of replaying the retired
// refresh token (the backend treats a replay past its grace window as theft
// and revokes the chain).
export const createTokenRefresher = (deps: TokenRefresherDeps) => {
	let inFlight: Promise<string | null> | null = null;
	const withLock = deps.withLock ?? noLock;

	const refreshOnce = async (): Promise<string | null> => {
		const stored = deps.readStored();
		if (!stored?.refresh_token) {
			return null;
		}
		if (stored.token && stored.token !== deps.currentAccessToken()) {
			deps.onSuccess(stored);
			return stored.token;
		}
		const envelope = await deps.post(stored.refresh_token);
		if (!envelope?.token) {
			return null;
		}
		deps.onSuccess(envelope);
		return envelope.token;
	};

	return (): Promise<string | null> => {
		if (inFlight) {
			return inFlight;
		}
		inFlight = withLock(refreshOnce)
			.catch(() => null)
			.finally(() => {
				inFlight = null;
			});
		return inFlight;
	};
};
