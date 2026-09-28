import type { LocalRouter } from './router';
import { normalizePath } from './router';
import { checkPersonaAccess } from './personaGate';
import { LocalHttpError, LocalRaw, type LocalActor, type LocalContext, type LocalResponse } from './types';

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

const JOURNALED_ACTOR_KINDS = new Set(['persona', 'plugin']);

/** A row is journaled for a non-GET write by persona/plugin, or a persona refusal (401/403) even on a GET. */
const journalWorthy = (actor: LocalActor | undefined, method: string, status: number): boolean => {
	if (!actor || !JOURNALED_ACTOR_KINDS.has(actor.kind)) return false;
	if (method !== 'GET') return true;
	return actor.kind === 'persona' && (status === 401 || status === 403);
};

const firstNumericParam = (params: Record<string, string>): string | null => {
	for (const value of Object.values(params)) {
		if (/^\d+$/.test(value)) return value;
	}
	return null;
};

/** Route pattern's first path segment as the entity, plus an id from the path or, for a create, the response body. */
const entityOf = (method: string, pattern: string, params: Record<string, string>, data: unknown) => {
	const entity = pattern.split('/')[0] || null;
	let entityId = firstNumericParam(params);
	if (entityId == null && method === 'POST' && data && typeof data === 'object' && 'id' in (data as object)) {
		const id = (data as Record<string, unknown>).id;
		entityId = id == null ? null : String(id);
	}
	return { entity, entityId };
};

const writeJournalRow = (
	ctx: LocalContext,
	actor: LocalActor,
	method: string,
	route: string,
	entity: string | null,
	entityId: string | null,
	status: number,
) =>
	ctx.db.execute(
		`INSERT INTO activity_log (at, actor_kind, actor_id, actor_name, method, route, entity, entity_id, status)
		 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		[ctx.now().toISOString(), actor.kind, actor.id, actor.name ?? null, method, route, entity, entityId, status],
	);

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
	const upperMethod = method.toUpperCase();
	const route = router.match(method, path);
	if (!route) return null;
	const actor = ctx.actor;
	try {
		if (actor?.kind === 'persona') await checkPersonaAccess(ctx, method, route.pattern);
		const data = await route.handler({
			method: upperMethod,
			path,
			params: route.params,
			query: queryOf(url, params),
			body: parseBody(body),
			ctx,
		});
		if (journalWorthy(actor, upperMethod, route.status)) {
			const { entity, entityId } = entityOf(upperMethod, route.pattern, route.params, data);
			try {
				await writeJournalRow(ctx, actor!, upperMethod, route.pattern, entity, entityId, route.status);
			} catch {
				// A persona's audit trail is part of the write, so a journal failure must surface, not vanish.
				if (actor!.kind === 'persona') return { status: 500, data: { message: 'Audit failed' } };
			}
		}
		if (data instanceof LocalRaw) return { status: route.status, data: data.body };
		// Same rule as the Java envelope filter: wrap unless the body already carries `data`.
		const wrapped =
			data && typeof data === 'object' && !Array.isArray(data) && 'data' in data
				? data
				: { data };
		return { status: route.status, data: wrapped };
	} catch (error) {
		if (error instanceof LocalHttpError) {
			if (journalWorthy(actor, upperMethod, error.status)) {
				const { entity, entityId } = entityOf(upperMethod, route.pattern, route.params, undefined);
				try {
					await writeJournalRow(ctx, actor!, upperMethod, route.pattern, entity, entityId, error.status);
				} catch {
					// Best effort: the response already carries the real refusal.
				}
			}
			return { status: error.status, data: { message: error.message } };
		}
		throw error;
	}
};
