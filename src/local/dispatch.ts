import type { LocalRouter } from './router';
import { normalizePath } from './router';
import { LocalHttpError, LocalRaw, type LocalContext, type LocalResponse } from './types';

const parseBody = (body: unknown) => {
	if (typeof body !== 'string') return body ?? {};
	try {
		return JSON.parse(body);
	} catch {
		return {};
	}
};

const queryOf = (url: string, params?: Record<string, any>) => {
	const query = new URLSearchParams(url.includes('?') ? url.slice(url.indexOf('?') + 1) : '');
	Object.entries(params ?? {}).forEach(([key, value]) => {
		if (value !== undefined && value !== null) query.set(key, String(value));
	});
	return query;
};

/**
 * Runs one request against the local API. Returns null when no local route matches, so the caller
 * decides what an unknown route means (it must never fall through to the server).
 */
export const dispatchLocal = async (
	router: LocalRouter,
	ctx: LocalContext,
	method: string,
	url: string,
	body?: unknown,
	params?: Record<string, any>,
): Promise<LocalResponse | null> => {
	const path = normalizePath(url);
	const route = router.match(method, path);
	if (!route) return null;
	try {
		const data = await route.handler({
			method: method.toUpperCase(),
			path,
			params: route.params,
			query: queryOf(url, params),
			body: parseBody(body),
			ctx,
		});
		if (data instanceof LocalRaw) return { status: route.status, data: data.body };
		// Same rule as the Java envelope filter: wrap unless the body already carries `data`.
		const wrapped =
			data && typeof data === 'object' && !Array.isArray(data) && 'data' in data
				? data
				: { data };
		return { status: route.status, data: wrapped };
	} catch (error) {
		if (error instanceof LocalHttpError) {
			return { status: error.status, data: { message: error.message } };
		}
		throw error;
	}
};
