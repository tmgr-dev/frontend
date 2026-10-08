import DOMPurify from 'dompurify';

export interface SanitizeOptions {
	/** false: content from a non-human author may not make the browser fetch anything. */
	remoteMedia?: boolean;
	/** Origins whose images are still shown when remoteMedia is false. */
	trustedOrigins?: string[];
}

const STRICT_CONFIG = {
	FORBID_TAGS: [
		'style',
		'svg',
		'math',
		'video',
		'audio',
		'source',
		'track',
		'picture',
		'object',
		'embed',
		'input',
		'link',
		'meta',
	],
	FORBID_ATTR: ['style', 'srcset', 'poster', 'background'],
};

const isTrusted = (src: string, origins: string[]): boolean => {
	try {
		const { origin } = new URL(src, window.location.href);
		return origin === window.location.origin || origins.includes(origin);
	} catch {
		return false;
	}
};

const toLink = (img: Element, doc: Document): Node => {
	const src = img.getAttribute('src') || '';
	const label = img.getAttribute('alt') || src;
	if (!/^https?:\/\//i.test(src) || img.closest('a')) {
		return doc.createTextNode(label);
	}
	const a = doc.createElement('a');
	a.setAttribute('href', src);
	a.setAttribute('target', '_blank');
	a.setAttribute('rel', 'noopener noreferrer');
	a.textContent = label;
	return a;
};

const sanitizeStrict = (html: string, origins: string[]): string => {
	const body = DOMPurify.sanitize(html, {
		...STRICT_CONFIG,
		RETURN_DOM: true,
	}) as unknown as HTMLElement;
	for (const img of Array.from(body.querySelectorAll('img'))) {
		if (!isTrusted(img.getAttribute('src') || '', origins)) {
			img.replaceWith(toLink(img, body.ownerDocument));
		}
	}
	return body.innerHTML;
};

export default function sanitizeHtml(
	html: string,
	options: SanitizeOptions = {},
): string {
	return options.remoteMedia === false
		? sanitizeStrict(html, options.trustedOrigins ?? [])
		: DOMPurify.sanitize(html);
}
