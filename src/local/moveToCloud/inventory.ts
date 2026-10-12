import type { LocalDb } from '../types';
import { resolveMime } from './model';

export interface LocalCounts {
	tasks: number;
	comments: number;
	files: number;
	pages: number;
	routines: number;
	agentRuns: number;
	statuses: number;
	categories: number;
}

export interface InventoryFile {
	id: number;
	name: string;
	key: string;
	size: number;
	mime: string;
}

export interface InventoryPersona {
	uuid: string;
	name: string;
	description: string | null;
	ownerUserId: number;
	ownerName: string;
}

export interface LocalInventory {
	schemaVersion: number;
	counts: LocalCounts;
	files: InventoryFile[];
	personas: InventoryPersona[];
	runningTimers: number;
}

const count = async (db: LocalDb, sql: string): Promise<number> =>
	Number((await db.select<{ n: number }>(sql))[0]?.n ?? 0);

export const readInventory = async (db: LocalDb): Promise<LocalInventory> => {
	const schema = await db.select<{ value: string }>(
		`SELECT value FROM meta WHERE key = 'schema_version'`,
	);
	const files = await db.select<any>(
		`SELECT id, name, file_path, mime_type, size FROM files ORDER BY id`,
	);
	const personas = await db.select<any>(
		`SELECT uuid, name, description, owner_user_id, owner_name FROM personas ORDER BY name`,
	);
	return {
		schemaVersion: Number(schema[0]?.value ?? 0),
		counts: {
			tasks: await count(
				db,
				`SELECT COUNT(*) AS n FROM tasks WHERE deleted_at IS NULL`,
			),
			comments: await count(
				db,
				`SELECT COUNT(*) AS n FROM comments WHERE deleted_at IS NULL`,
			),
			files: files.length,
			pages: await count(
				db,
				`SELECT COUNT(*) AS n FROM pages WHERE deleted_at IS NULL`,
			),
			routines: await count(
				db,
				`SELECT COUNT(*) AS n FROM routines WHERE deleted_at IS NULL`,
			),
			agentRuns: await count(db, `SELECT COUNT(*) AS n FROM agent_work_runs`),
			statuses: await count(db, `SELECT COUNT(*) AS n FROM statuses`),
			categories: await count(
				db,
				`SELECT COUNT(*) AS n FROM categories WHERE deleted_at IS NULL`,
			),
		},
		files: files.map((row) => ({
			id: Number(row.id),
			name: String(row.name),
			key: String(row.file_path),
			size: Number(row.size ?? 0),
			mime: resolveMime(row.mime_type, String(row.file_path)),
		})),
		personas: personas.map((row) => ({
			uuid: String(row.uuid),
			name: String(row.name),
			description: row.description ?? null,
			ownerUserId: Number(row.owner_user_id),
			ownerName: String(row.owner_name ?? ''),
		})),
		runningTimers: await count(
			db,
			`SELECT COUNT(*) AS n FROM tasks WHERE start_time > 0 AND deleted_at IS NULL`,
		),
	};
};
