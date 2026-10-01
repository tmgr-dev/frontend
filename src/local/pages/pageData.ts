import type { LocalRouter } from '../router';
import { LocalHttpError, type LocalContext, type LocalDb } from '../types';

const QUOTA_BYTES = 5 * 1024 * 1024;
const MAX_KEYS = 1000;
const ROW_OVERHEAD = 64;
const MAX_PAGE_IDS = 500;

const byteLength = (value: string): number =>
	new TextEncoder().encode(value).byteLength;

/** Plugin data of permanently deleted pages; soft-deleted (trashed) pages keep theirs so a restore loses nothing. */
export const deletePluginPageData = async (db: LocalDb, pageIds: number[]) => {
	if (!pageIds.length) return;
	await db.execute(
		`DELETE FROM plugin_page_data WHERE page_id IN (${pageIds
			.map(() => '?')
			.join(',')})`,
		pageIds,
	);
};

const requireOwnPlugin = (ctx: LocalContext, pluginId: string) => {
	if (
		ctx.actor?.kind === 'plugin' &&
		(ctx.actor.ownerId ?? ctx.actor.id) !== pluginId
	)
		throw new LocalHttpError(403, 'Plugin data belongs to another plugin');
};

const requireActivePage = async (ctx: LocalContext, id: number) => {
	const [row] = await ctx.db.select<any>(
		`SELECT id FROM pages WHERE id = ? AND deleted_at IS NULL`,
		[id],
	);
	if (!row) throw new LocalHttpError(404, 'Page not found');
};

export const addPageDataRoutes = (router: LocalRouter) =>
	router
		.add(
			'GET',
			'plugins/:pid/pages/:id(\\d+)/data/:key',
			async ({ ctx, params }) => {
				requireOwnPlugin(ctx, params.pid);
				await requireActivePage(ctx, Number(params.id));
				const [row] = await ctx.db.select<{ value: string }>(
					`SELECT value FROM plugin_page_data WHERE plugin_id = ? AND page_id = ? AND key = ?`,
					[params.pid, Number(params.id), params.key],
				);
				return { value: row?.value ?? null };
			},
		)
		.add(
			'PUT',
			'plugins/:pid/pages/:id(\\d+)/data/:key',
			async ({ ctx, params, body }) => {
				requireOwnPlugin(ctx, params.pid);
				const pageId = Number(params.id);
				const value = typeof body?.value === 'string' ? body.value : null;
				if (value === null)
					throw new LocalHttpError(422, 'value must be a JSON string');
				const { rowsAffected } = await ctx.db.execute(
					`INSERT INTO plugin_page_data (plugin_id, page_id, key, value, updated_at)
				 SELECT ?, ?, ?, ?, ?
				 WHERE EXISTS (SELECT 1 FROM pages WHERE id = ? AND deleted_at IS NULL)
				   AND ((SELECT COALESCE(SUM(LENGTH(CAST(key AS BLOB)) + LENGTH(CAST(value AS BLOB)) + ${ROW_OVERHEAD}), 0)
				         FROM plugin_page_data WHERE plugin_id = ? AND NOT (page_id = ? AND key = ?))
				        + ? <= ${QUOTA_BYTES})
				   AND (SELECT COUNT(*) FROM plugin_page_data WHERE plugin_id = ? AND NOT (page_id = ? AND key = ?))
				        < ${MAX_KEYS}
				 ON CONFLICT (plugin_id, page_id, key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at`,
					[
						params.pid,
						pageId,
						params.key,
						value,
						ctx.now().toISOString(),
						pageId,
						params.pid,
						pageId,
						params.key,
						byteLength(params.key) + byteLength(value) + ROW_OVERHEAD,
						params.pid,
						pageId,
						params.key,
					],
				);
				if (!rowsAffected) {
					await requireActivePage(ctx, pageId);
					throw new LocalHttpError(
						413,
						'plugin page data is full (5 MB or 1000 keys)',
					);
				}
				return { success: true };
			},
		)
		.add(
			'DELETE',
			'plugins/:pid/pages/:id(\\d+)/data/:key',
			async ({ ctx, params }) => {
				requireOwnPlugin(ctx, params.pid);
				await requireActivePage(ctx, Number(params.id));
				await ctx.db.execute(
					`DELETE FROM plugin_page_data WHERE plugin_id = ? AND page_id = ? AND key = ?`,
					[params.pid, Number(params.id), params.key],
				);
				return { success: true };
			},
		)
		.add(
			'POST',
			'plugins/:pid/page-data/query',
			async ({ ctx, params, body }) => {
				requireOwnPlugin(ctx, params.pid);
				const pageIds = Array.isArray(body?.page_ids)
					? body.page_ids.map(Number).slice(0, MAX_PAGE_IDS)
					: [];
				const key = String(body?.key ?? '');
				if (!pageIds.length || !key) return {};
				const rows = await ctx.db.select<{ page_id: number; value: string }>(
					`SELECT page_id, value FROM plugin_page_data WHERE plugin_id = ? AND key = ? AND page_id IN (${pageIds
						.map(() => '?')
						.join(',')})`,
					[params.pid, key, ...pageIds],
				);
				const result: Record<number, string> = {};
				for (const row of rows) result[row.page_id] = row.value;
				return result;
			},
		);
