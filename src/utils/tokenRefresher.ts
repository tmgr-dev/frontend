export interface TokenEnvelope {
	token: string;
	refresh_token?: string;
	[key: string]: unknown;
}

export interface TokenRefresherDeps {
	getRefreshToken: () => string | undefined;
	post: (refreshToken: string) => Promise<TokenEnvelope>;
	onSuccess: (envelope: TokenEnvelope) => void;
}

const AUTH_URL_PATTERN = /^\/?auth\/(login|register|refresh)(\/|$)/;

export const isAuthUrl = (url: string | undefined): boolean =>
	!!url && AUTH_URL_PATTERN.test(url);

// Deliberately NOT requestCache.getOrFetch: logout() calls
// requestCache.clear(), which must never cancel an in-flight token refresh,
// and a failed refresh resolves null instead of rejecting.
export const createTokenRefresher = (deps: TokenRefresherDeps) => {
	let inFlight: Promise<string | null> | null = null;

	return (): Promise<string | null> => {
		if (inFlight) {
			return inFlight;
		}
		const refreshToken = deps.getRefreshToken();
		if (!refreshToken) {
			return Promise.resolve(null);
		}
		inFlight = deps
			.post(refreshToken)
			.then((envelope) => {
				if (!envelope?.token) {
					return null;
				}
				deps.onSuccess(envelope);
				return envelope.token;
			})
			.catch(() => null)
			.finally(() => {
				inFlight = null;
			});
		return inFlight;
	};
};
