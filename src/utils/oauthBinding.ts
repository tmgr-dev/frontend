const storageKey = (provider: string): string => `oauth.binding.${provider}`;

const toBase64Url = (bytes: Uint8Array): string =>
	btoa(String.fromCharCode(...bytes))
		.replace(/\+/g, '-')
		.replace(/\//g, '_')
		.replace(/=+$/, '');

/**
 * Random per-tab secret sent with the OAuth kickoff and echoed back on the
 * callback, so a callback started in another browser is rejected.
 */
export const createOAuthBinding = (provider: string): string => {
	const binding = toBase64Url(crypto.getRandomValues(new Uint8Array(32)));
	try {
		sessionStorage.setItem(storageKey(provider), binding);
	} catch {}
	return binding;
};

/** Single-use: the stored binding is removed on read. */
export const takeOAuthBinding = (provider: string): string | undefined => {
	try {
		return sessionStorage.getItem(storageKey(provider)) ?? undefined;
	} catch {
		return undefined;
	} finally {
		try {
			sessionStorage.removeItem(storageKey(provider));
		} catch {}
	}
};
