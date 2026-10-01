import { parseJson } from '../serialize';
import type { LocalContext } from '../types';
import { sections } from './markdown';

export interface PageRow {
	id: number;
	parent_id: number | null;
	title: string;
	slug: string;
	type: string;
	body: string;
	properties: string;
	version: number;
	author_id: number;
	author_kind: string;
	author_ref: string;
	updated_by_id: number;
	updated_by_kind: string;
	updated_by_ref: string;
	position: number;
	pinned: number;
	created_at: string;
	updated_at: string;
	deleted_at: string | null;
}

export interface VersionRow {
	page_id: number;
	version: number;
	title: string;
	body: string;
	properties: string;
	author_id: number;
	author_kind: string;
	author_ref: string;
	summary: string | null;
	created_at: string;
}

export interface FileRow {
	id: number;
	page_id: number | null;
	name: string;
	file_path: string;
	mime_type: string | null;
	size: number | null;
	created_at: string;
}

export const SUMMARY_COLUMNS = `id, parent_id, title, slug, type, '' AS body, '{}' AS properties, version,
	author_id, author_kind, author_ref, updated_by_id, updated_by_kind, updated_by_ref, position, pinned,
	created_at, updated_at, deleted_at`;

export const summaryJson = (row: PageRow) => ({
	id: row.id,
	title: row.title,
	slug: row.slug,
	type: row.type,
	parent_id: row.parent_id,
	position: row.position,
	pinned: !!row.pinned,
	updated_at: row.updated_at,
});

export const trashJson = (row: PageRow) => ({ ...summaryJson(row), deleted_at: row.deleted_at });

export const propertiesOf = (raw: string | null | undefined): Record<string, any> =>
	parseJson<Record<string, any>>(raw, {});

export type Authors = Map<string, { id: string | number; name: string | null }>;

const authorKey = (kind: string, ref: string) => `${kind}:${ref}`;

export const authorsFor = async (
	ctx: LocalContext,
	refs: { kind: string; ref: string }[],
): Promise<Authors> => {
	const names: Authors = new Map();
	const personas = [...new Set(refs.filter((r) => r.kind === 'persona').map((r) => r.ref))];
	if (personas.length) {
		const rows = await ctx.db.select<{ uuid: string; name: string }>(
			`SELECT uuid, name FROM personas WHERE uuid IN (${personas.map(() => '?').join(',')})`,
			personas,
		);
		rows.forEach((row) => names.set(authorKey('persona', row.uuid), { id: row.uuid, name: row.name }));
	}
	return names;
};

export const authorJson = (ctx: LocalContext, kind: string, ref: string, names: Authors) => {
	if (kind === 'user') {
		return { kind: 'user', id: Number.isFinite(Number(ref)) ? Number(ref) : ref, name: ctx.user.name };
	}
	const known = names.get(authorKey(kind, ref));
	return {
		kind,
		id: ref,
		name: known?.name ?? (ctx.actor && ctx.actor.kind === kind && ctx.actor.id === ref ? ctx.actor.name : ref),
		owner: { id: String(ctx.user.id), name: ctx.user.name },
	};
};

export const fileJson = (row: FileRow, ctx: LocalContext) => ({
	id: row.id,
	page_id: row.page_id,
	user_id: ctx.user.id,
	workspace_id: ctx.workspace.id,
	name: row.name,
	original_name: row.name,
	file_path: row.file_path,
	mime_type: row.mime_type,
	size: row.size,
	created_at: row.created_at,
});

export const pageJson = (
	row: PageRow,
	ctx: LocalContext,
	names: Authors,
	backlinks: PageRow[],
	files: FileRow[],
) => ({
	...summaryJson(row),
	workspace_id: ctx.workspace.id,
	body: row.body,
	properties: propertiesOf(row.properties),
	version: row.version,
	author: authorJson(ctx, row.author_kind, row.author_ref, names),
	updated_by: authorJson(ctx, row.updated_by_kind, row.updated_by_ref, names),
	created_at: row.created_at,
	deleted_at: row.deleted_at,
	backlinks: backlinks.map(summaryJson),
	sections: sections(row.body).map((s) => ({ id: s.id, owner: s.owner, heading: s.heading })),
	files: files.map((f) => fileJson(f, ctx)),
	following: false,
});

export const versionJson = (row: VersionRow, ctx: LocalContext, names: Authors) => ({
	version: row.version,
	title: row.title,
	author: authorJson(ctx, row.author_kind, row.author_ref, names),
	summary: row.summary,
	created_at: row.created_at,
});

export const snapshotJson = (row: VersionRow, ctx: LocalContext, names: Authors) => ({
	...versionJson(row, ctx, names),
	body: row.body,
	properties: propertiesOf(row.properties),
});
