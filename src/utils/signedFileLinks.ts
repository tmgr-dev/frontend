export interface SignedLink {
	/** API-relative URL carrying the signature, as issued by `GET /files/{id}/signed-url`. */
	url: string;
	/** Epoch milliseconds after which the signature is refused. */
	expiresAt: number;
}

export interface SignedLinkCacheOptions {
	/** Resolves to null when this deployment does not sign links at all. */
	sign: (fileId: number) => Promise<SignedLink | null>;
	now?: () => number;
}

export interface SignedLinkCache {
	get: (fileId: number) => Promise<string | null>;
	invalidate: (fileId: number) => void;
}

/** Re-sign this long before the deadline so a link never expires mid-render. */
export const RENEW_MARGIN_MS = 5 * 60 * 1000;

export const absoluteLinkUrl = (url: string, apiBaseUrl: string): string =>
	new URL(url, apiBaseUrl).toString();

/**
 * Keeps one signature per file for as long as it is good for. A deployment without
 * `FILE_LINK_SECRET` answers 503 once and is then left alone for the rest of the session,
 * so every caller falls back to fetching the bytes instead of asking again per image.
 */
export const createSignedLinkCache = (
	options: SignedLinkCacheOptions,
): SignedLinkCache => {
	const now = options.now ?? (() => Date.now());
	const links = new Map<number, SignedLink>();
	const inFlight = new Map<number, Promise<string | null>>();
	let signingAvailable = true;

	const request = async (fileId: number): Promise<string | null> => {
		const link = await options.sign(fileId);

		if (!link) {
			signingAvailable = false;

			return null;
		}

		links.set(fileId, link);

		return link.url;
	};

	return {
		async get(fileId: number): Promise<string | null> {
			if (!signingAvailable) {
				return null;
			}

			const cached = links.get(fileId);

			if (cached && cached.expiresAt - now() > RENEW_MARGIN_MS) {
				return cached.url;
			}

			const pending = inFlight.get(fileId);

			if (pending) {
				return pending;
			}

			const promise = request(fileId).finally(() => inFlight.delete(fileId));

			inFlight.set(fileId, promise);

			return promise;
		},
		invalidate(fileId: number) {
			links.delete(fileId);
		},
	};
};
