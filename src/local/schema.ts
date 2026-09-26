import type { LocalDb } from './types';

export interface Migration {
	version: number;
	statements: string[];
}

export const MIGRATIONS: Migration[] = [
	{
		version: 1,
		statements: [
			`CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT)`,
			`CREATE TABLE IF NOT EXISTS statuses (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				name TEXT NOT NULL,
				type TEXT NOT NULL DEFAULT 'active',
				color TEXT,
				sort_order INTEGER NOT NULL DEFAULT 0,
				created_at TEXT NOT NULL,
				updated_at TEXT NOT NULL
			)`,
			`CREATE TABLE IF NOT EXISTS categories (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				title TEXT NOT NULL,
				slug TEXT,
				code TEXT,
				parent_id INTEGER REFERENCES categories(id) ON DELETE SET NULL,
				settings TEXT NOT NULL DEFAULT '[]',
				created_at TEXT NOT NULL,
				updated_at TEXT NOT NULL,
				deleted_at TEXT
			)`,
			`CREATE TABLE IF NOT EXISTS tasks (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				title TEXT NOT NULL,
				description TEXT,
				description_json TEXT,
				status_id INTEGER REFERENCES statuses(id) ON DELETE SET NULL,
				project_category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL,
				category_tasks_sequence_id INTEGER,
				sort_order INTEGER,
				priority TEXT NOT NULL DEFAULT 'medium',
				common_time INTEGER NOT NULL DEFAULT 0,
				start_time INTEGER NOT NULL DEFAULT 0,
				end_time INTEGER,
				approximately_time INTEGER NOT NULL DEFAULT 0,
				checkpoints TEXT,
				settings TEXT NOT NULL DEFAULT '[]',
				expired_at TEXT,
				created_at TEXT NOT NULL,
				updated_at TEXT NOT NULL,
				deleted_at TEXT
			)`,
			`CREATE INDEX IF NOT EXISTS tasks_status_idx ON tasks (status_id, deleted_at)`,
			`CREATE INDEX IF NOT EXISTS tasks_category_idx ON tasks (project_category_id, category_tasks_sequence_id)`,
			`CREATE INDEX IF NOT EXISTS tasks_running_idx ON tasks (start_time)`,
			`CREATE TABLE IF NOT EXISTS comments (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
				message TEXT NOT NULL,
				created_at TEXT NOT NULL,
				updated_at TEXT NOT NULL,
				deleted_at TEXT
			)`,
			`CREATE INDEX IF NOT EXISTS comments_task_idx ON comments (task_id)`,
		],
	},
	{
		version: 2,
		statements: [
			`CREATE TABLE IF NOT EXISTS files (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
				name TEXT NOT NULL,
				file_path TEXT NOT NULL UNIQUE,
				mime_type TEXT,
				size INTEGER,
				created_at TEXT NOT NULL
			)`,
			`CREATE INDEX IF NOT EXISTS files_task_idx ON files (task_id)`,
		],
	},
];

export const LATEST_SCHEMA = MIGRATIONS[MIGRATIONS.length - 1].version;

const DEFAULT_STATUSES: [string, string, string][] = [
	['Backlog', 'default', '#64748b'],
	['In progress', 'active', '#3b82f6'],
	['Done', 'completed', '#22c55e'],
	['Archived', 'archived', '#374151'],
];

export const readSchemaVersion = async (db: LocalDb): Promise<number> => {
	const tables = await db.select<{ name: string }>(
		`SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'meta'`,
	);
	if (!tables.length) return 0;
	const rows = await db.select<{ value: string }>(
		`SELECT value FROM meta WHERE key = 'schema_version'`,
	);
	return rows.length ? Number(rows[0].value) : 0;
};

/**
 * Brings the database to the latest schema. Every statement is idempotent, so a migration cut
 * short (the app quit mid-way) simply runs again on the next open.
 */
export const migrate = async (
	db: LocalDb,
	now: string,
	beforeMigration?: (from: number) => Promise<void>,
): Promise<number> => {
	const current = await readSchemaVersion(db);
	if (current > LATEST_SCHEMA) {
		throw new Error(
			`This workspace was written by a newer TMGR (schema ${current}); update the app to open it.`,
		);
	}
	if (current > 0 && current < LATEST_SCHEMA && beforeMigration) await beforeMigration(current);
	for (const migration of MIGRATIONS.filter((m) => m.version > current)) {
		for (const statement of migration.statements) {
			await db.execute(statement);
		}
		await db.execute(
			`INSERT INTO meta (key, value) VALUES ('schema_version', ?)
			 ON CONFLICT(key) DO UPDATE SET value = excluded.value`,
			[String(migration.version)],
		);
	}
	const statuses = await db.select<{ n: number }>(`SELECT COUNT(*) AS n FROM statuses`);
	if (!Number(statuses[0]?.n)) {
		for (const [index, [name, type, color]] of DEFAULT_STATUSES.entries()) {
			await db.execute(
				`INSERT INTO statuses (name, type, color, sort_order, created_at, updated_at)
				 VALUES (?, ?, ?, ?, ?, ?)`,
				[name, type, color, index + 1, now, now],
			);
		}
	}
	return LATEST_SCHEMA;
};
