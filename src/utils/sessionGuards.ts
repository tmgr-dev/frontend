// Guards that keep a 401 from tearing down a session it does not belong to.

const SOCIAL_CALLBACK_PATH = /^\/login\/[^/]+/;

/** `/login/:platform` — the OAuth callback page that exchanges the code and owns the session it creates. */
export const isSocialCallbackPath = (pathname: string): boolean =>
	SOCIAL_CALLBACK_PATH.test(pathname);

const bearerToken = (authorizationHeader: string | undefined): string =>
	(authorizationHeader || '').replace(/^Bearer\s+/i, '');

/**
 * True when a request was sent with the token that is still current. A 401 for
 * a request that carried an older (or no) token belongs to a session that has
 * already been replaced — e.g. a boot-time call fired with a stale token while
 * the social callback exchanged the code — and must not log the new one out.
 */
export const wasSentWithCurrentToken = (
	authorizationHeader: string | undefined,
	currentToken: string | null | undefined,
): boolean => bearerToken(authorizationHeader) === (currentToken || '');

/**
 * True when a 401 came back for a request that carried a token another tab
 * has since rotated (the store already holds the newer one): replay the
 * request once with the current token instead of dropping it. False when
 * the session is gone (logout elsewhere) or the replay already happened.
 */
export const shouldReplayWithCurrentToken = (
	authorizationHeader: string | undefined,
	currentToken: string | null | undefined,
	alreadyReplayed: boolean,
): boolean =>
	!alreadyReplayed &&
	!!currentToken &&
	bearerToken(authorizationHeader) !== currentToken;
