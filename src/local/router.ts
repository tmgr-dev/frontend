import type { LocalRequest } from './types';

type Handler = (req: LocalRequest) => Promise<any> | any;

interface Route {
	method: string;
	pattern: string;
	regex: RegExp;
	keys: string[];
	handler: Handler;
	status: number;
}

export const normalizePath = (url: string): string =>
	url
		.split('?')[0]
		.replace(/^https?:\/\/[^/]+/, '')
		.replace(/^\/?(api\/)?/, '')
		.replace(/^\/+|\/+$/g, '');

/**
 * Tiny method + path router. Patterns use `:name` for a path segment, and `:name(\\d+)` for a
 * constrained one. The first matching route wins, so specific routes go first.
 */
export class LocalRouter {
	private routes: Route[] = [];

	add(method: string, pattern: string, handler: Handler, status = 200): this {
		const keys: string[] = [];
		const source = pattern
			.split('/')
			.map((segment) => {
				const match = /^:(\w+)(\((.+)\))?$/.exec(segment);
				if (!match) return segment.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
				keys.push(match[1]);
				return `(${match[3] ?? '[^/]+'})`;
			})
			.join('/');
		this.routes.push({
			method: method.toUpperCase(),
			pattern,
			regex: new RegExp(`^${source}$`),
			keys,
			handler,
			status,
		});
		return this;
	}

	match(
		method: string,
		path: string,
	): { handler: Handler; params: Record<string, string>; status: number; pattern: string } | null {
		const upper = method.toUpperCase();
		for (const route of this.routes) {
			if (route.method !== upper) continue;
			const found = route.regex.exec(path);
			if (!found) continue;
			const params: Record<string, string> = {};
			route.keys.forEach((key, i) => {
				params[key] = decodeURIComponent(found[i + 1]);
			});
			return { handler: route.handler, params, status: route.status, pattern: route.pattern };
		}
		return null;
	}
}
