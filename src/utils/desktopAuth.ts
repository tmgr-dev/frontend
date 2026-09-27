export type DesktopAuthProvider = 'google' | 'apple' | 'github' | 'telegram';

export interface PendingDesktopAuth {
	provider: DesktopAuthProvider;
	state: string;
	verifier: string;
	createdAt: number;
}

export type AuthCallback =
	| { type: 'auth'; code: string; state: string }
	| { type: 'auth'; error: string };

export type RelayResult = { code: string; state: string } | { error: string };

type KeyValueStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;

export const PENDING_TTL_MS = 10 * 60 * 1000;
const PENDING_KEY = 'desktop.auth.pending';
const LATEST_KEY = 'desktop.auth.latest';
const CALLBACK_PREFIX = 'tmgr://auth/callback';
const DESKTOP_STATE = /^desktop\.([A-Za-z0-9_-]{43})$/;
const CODE = /^[A-Za-z0-9_-]{43}$/;
const STATE = /^[A-Za-z0-9_-]{16,128}$/;
const ERROR = /^[a-z]{1,20}$/;

const base64Url = (bytes: Uint8Array): string =>
	btoa(String.fromCharCode(...bytes))
		.replace(/\+/g, '-')
		.replace(/\//g, '_')
		.replace(/=+$/, '');

const randomToken = (crypto: Crypto): string =>
	base64Url(crypto.getRandomValues(new Uint8Array(32)));

export const createPkcePair = async (
	crypto: Crypto = globalThis.crypto,
): Promise<{ verifier: string; challenge: string }> => {
	if (!crypto?.subtle || !crypto.getRandomValues) {
		throw new Error('Sign-in needs WebCrypto, which requires a secure context');
	}
	const verifier = randomToken(crypto);
	const digest = await crypto.subtle.digest(
		'SHA-256',
		new TextEncoder().encode(verifier),
	);
	return { verifier, challenge: base64Url(new Uint8Array(digest)) };
};

export const beginDesktopAuth = async (
	provider: DesktopAuthProvider,
	apiBaseUrl: string,
	storage: KeyValueStorage = localStorage,
	now: () => number = Date.now,
	crypto: Crypto = globalThis.crypto,
): Promise<string> => {
	const { verifier, challenge } = await createPkcePair(crypto);
	const state = randomToken(crypto);
	const pending: PendingDesktopAuth = {
		provider,
		state,
		verifier,
		createdAt: now(),
	};
	storage.setItem(PENDING_KEY, JSON.stringify(pending));
	storage.setItem(LATEST_KEY, state);
	return `${apiBaseUrl}auth/login/desktop/${provider}?code_challenge=${challenge}&state=${state}`;
};

const readPending = (storage: KeyValueStorage): PendingDesktopAuth | null => {
	try {
		const pending = JSON.parse(storage.getItem(PENDING_KEY) || 'null');
		return pending && typeof pending.state === 'string' ? pending : null;
	} catch {
		return null;
	}
};

export const hasPendingDesktopAuth = (
	storage: KeyValueStorage = localStorage,
	now: () => number = Date.now,
): boolean => {
	const pending = readPending(storage);
	return !!pending && now() - pending.createdAt < PENDING_TTL_MS;
};

/** False once a newer attempt was started: an older one's late result must not replace it. */
export const isLatestDesktopAuth = (
	state: string,
	storage: KeyValueStorage = localStorage,
): boolean => storage.getItem(LATEST_KEY) === state;

export const clearPendingDesktopAuth = (
	storage: KeyValueStorage = localStorage,
): void => storage.removeItem(PENDING_KEY);

/** The attempt this callback belongs to, consumed; null for a foreign, stale or replayed callback. */
export const takePendingDesktopAuth = (
	state: string | undefined,
	storage: KeyValueStorage = localStorage,
	now: () => number = Date.now,
): PendingDesktopAuth | null => {
	const pending = readPending(storage);
	if (!pending) return null;
	if (now() - pending.createdAt >= PENDING_TTL_MS) {
		clearPendingDesktopAuth(storage);
		return null;
	}
	if (!state || pending.state !== state) return null;
	clearPendingDesktopAuth(storage);
	return pending;
};

const parseResult = (search: string): RelayResult | null => {
	const params = new URLSearchParams(search);
	const code = params.get('code');
	const state = params.get('state');
	const error = params.get('error');
	if (code !== null || state !== null) {
		return code && state && CODE.test(code) && STATE.test(state)
			? { code, state }
			: null;
	}
	return error && ERROR.test(error) ? { error } : null;
};

export const parseAuthCallback = (url: string): AuthCallback | null => {
	const trimmed = url.trim();
	if (!trimmed.startsWith(`${CALLBACK_PREFIX}?`)) return null;
	const result = parseResult(trimmed.slice(CALLBACK_PREFIX.length + 1));
	return result ? { type: 'auth', ...result } : null;
};

export const desktopTxFromState = (state: unknown): string | null =>
	typeof state === 'string' ? (DESKTOP_STATE.exec(state)?.[1] ?? null) : null;

export const parseRelayFragment = (hash: string): RelayResult | null =>
	parseResult(hash.replace(/^#/, ''));

export const buildDesktopCallbackUrl = (result: RelayResult): string =>
	'error' in result
		? `${CALLBACK_PREFIX}?error=${result.error}`
		: `${CALLBACK_PREFIX}?code=${result.code}&state=${result.state}`;

export const relayReturnHash = (result: RelayResult): string =>
	'error' in result
		? `#error=${result.error}`
		: `#code=${result.code}&state=${result.state}`;

/** Top-level navigation (not XHR) so the API sees its browser-binding cookie. */
export const desktopCompleteUrl = (
	apiBaseUrl: string,
	provider: DesktopAuthProvider,
	params: Record<string, string | number | boolean> = {},
): string => {
	const query = new URLSearchParams(
		Object.entries(params).map(([key, value]) => [key, String(value)]),
	).toString();
	const url = `${apiBaseUrl}auth/login/desktop/${provider}/complete`;
	return query ? `${url}?${query}` : url;
};
