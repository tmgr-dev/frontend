export const DEFAULT_ALLOWED: RegExp[] = [
	/^\/assets\//,
	/^\/registerSW\.js$/,
	/^\/sw\.js$/,
	/^\/workbox-[^/]*\.js$/,
];

const MAX_PATH = 40;

const parse = (href: string): URL | null => {
	try {
		return new URL(href);
	} catch {
		return null;
	}
};

export function findInjectedScripts(
	srcs: string[],
	pageHref: string,
	allowed: RegExp[] = DEFAULT_ALLOWED,
): string[] {
	const page = parse(pageHref);
	if (!page || (page.protocol !== 'http:' && page.protocol !== 'https:'))
		return [];
	const found: string[] = [];
	for (const src of srcs) {
		const url = src ? parse(src) : null;
		if (
			!url ||
			url.protocol !== page.protocol ||
			url.host !== page.host ||
			allowed.some((re) => re.test(url.pathname))
		)
			continue;
		const path = url.pathname;
		found.push(path.length > MAX_PATH ? `${path.slice(0, MAX_PATH)}…` : path);
	}
	return found;
}
