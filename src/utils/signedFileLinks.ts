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
	context?: () => string;
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

	let context = options.context?.();
	let generation = 0;

	return {
		async get(fileId: number): Promise<string | null> {
			if (context !== options.context?.()) {
				context = options.context?.();
				generation++;
				links.clear();
				inFlight.clear();
				signingAvailable = true;
			}
			for (const [id, link] of links)
				if (link.expiresAt <= now()) links.delete(id);
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

			const epoch = generation;
			const promise = options
				.sign(fileId)
				.then((link) => {
					if (epoch !== generation || context !== options.context?.())
						throw new DOMException('File context changed', 'AbortError');
					if (inFlight.get(fileId) === promise) {
						if (!link) signingAvailable = false;
						else {
							links.delete(fileId);
							links.set(fileId, link);
							while (links.size > 200)
								links.delete(links.keys().next().value as number);
						}
					}
					return link?.url ?? null;
				})
				.finally(() => {
					if (inFlight.get(fileId) === promise) inFlight.delete(fileId);
				});

			inFlight.set(fileId, promise);

			return promise;
		},
		invalidate(fileId: number) {
			links.delete(fileId);
			inFlight.delete(fileId);
		},
	};
};
