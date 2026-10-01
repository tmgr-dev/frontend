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
	{
		version: 3,
		statements: [
			`CREATE TABLE IF NOT EXISTS plugin_kv (
				plugin_id TEXT NOT NULL,
				key TEXT NOT NULL,
				value TEXT NOT NULL,
				updated_at TEXT NOT NULL,
				PRIMARY KEY (plugin_id, key)
			)`,
		],
	},
	{
		version: 4,
		statements: [
			`CREATE TABLE IF NOT EXISTS routines (
				id INTEGER PRIMARY KEY,
				title TEXT NOT NULL,
				description TEXT,
				routine_category TEXT,
				priority TEXT,
				approximately_time INTEGER,
				scheduled_date TEXT,
				scheduled_time TEXT,
				settings TEXT NOT NULL DEFAULT '[]',
				archived_at TEXT,
				created_at TEXT NOT NULL,
				updated_at TEXT NOT NULL,
				deleted_at TEXT
			)`,
			`CREATE TABLE IF NOT EXISTS routine_patterns (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				routine_id INTEGER NOT NULL UNIQUE REFERENCES routines(id) ON DELETE CASCADE,
				frequency TEXT,
				interval INTEGER NOT NULL DEFAULT 1,
				day_of_frequency INTEGER,
				month INTEGER,
				days_of_week TEXT,
				start_at TEXT,
				end_at TEXT,
				occurrences INTEGER,
				scheduled_time TEXT,
				duration_min INTEGER,
				reminder_min INTEGER,
				created_at TEXT NOT NULL,
				updated_at TEXT NOT NULL
			)`,
			`CREATE TABLE IF NOT EXISTS routine_instances (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				routine_id INTEGER NOT NULL REFERENCES routines(id) ON DELETE CASCADE,
				scheduled_for TEXT NOT NULL,
				status TEXT NOT NULL DEFAULT 'PENDING',
				completed_at TEXT,
				skipped_at TEXT,
				created_at TEXT NOT NULL,
				updated_at TEXT NOT NULL
			)`,
			`CREATE INDEX IF NOT EXISTS routine_instances_routine_idx ON routine_instances (routine_id, scheduled_for)`,
		],
	},
	{
		version: 5,
		statements: [
			`ALTER TABLE comments ADD COLUMN author_kind TEXT NOT NULL DEFAULT 'user'`,
			`ALTER TABLE comments ADD COLUMN author_id TEXT`,
			`ALTER TABLE comments ADD COLUMN author_name TEXT`,
			`CREATE TABLE IF NOT EXISTS comment_reactions (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				comment_id INTEGER NOT NULL REFERENCES comments(id) ON DELETE CASCADE,
				emoji TEXT NOT NULL,
				actor_kind TEXT NOT NULL,
				actor_id TEXT NOT NULL,
				created_at TEXT NOT NULL,
				UNIQUE (comment_id, emoji, actor_kind, actor_id)
			)`,
			`CREATE INDEX IF NOT EXISTS comment_reactions_comment_idx ON comment_reactions (comment_id)`,
			`CREATE TABLE IF NOT EXISTS task_relation_types (
				id INTEGER PRIMARY KEY,
				name TEXT NOT NULL UNIQUE
			)`,
			`INSERT OR IGNORE INTO task_relation_types (id, name) VALUES
				(1, 'blocks'),
				(2, 'is blocked by'),
				(3, 'relates to'),
				(4, 'duplicates'),
				(5, 'is duplicated by'),
				(6, 'depends on'),
				(7, 'is dependency of')`,
			`CREATE TABLE IF NOT EXISTS task_relations (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
				related_task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
				relation_type_id INTEGER NOT NULL REFERENCES task_relation_types(id),
				created_at TEXT NOT NULL,
				UNIQUE (task_id, related_task_id, relation_type_id)
			)`,
			`CREATE INDEX IF NOT EXISTS task_relations_task_idx ON task_relations (task_id)`,
		],
	},
	{
		version: 6,
		statements: [
			`CREATE TABLE IF NOT EXISTS plugin_task_data (
				plugin_id TEXT NOT NULL,
				task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
				key TEXT NOT NULL,
				value TEXT NOT NULL,
				updated_at TEXT NOT NULL,
				PRIMARY KEY (plugin_id, task_id, key)
			)`,
			`CREATE INDEX IF NOT EXISTS plugin_task_data_plugin_key_idx ON plugin_task_data (plugin_id, key)`,
			`CREATE TABLE IF NOT EXISTS agent_work_runs (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
				agent TEXT NOT NULL,
				model TEXT,
				session_id TEXT,
				branch TEXT,
				status TEXT NOT NULL DEFAULT 'running',
				started_at TEXT NOT NULL,
				ended_at TEXT,
				duration_seconds INTEGER,
				summary TEXT,
				pr_url TEXT,
				commits TEXT,
				tests TEXT,
				actor_kind TEXT NOT NULL,
				actor_id TEXT NOT NULL,
				version INTEGER NOT NULL DEFAULT 1,
				created_at TEXT NOT NULL,
				updated_at TEXT NOT NULL
			)`,
			`CREATE INDEX IF NOT EXISTS agent_work_runs_task_idx ON agent_work_runs (task_id)`,
		],
	},
	{
		version: 7,
		statements: [
			`CREATE TABLE IF NOT EXISTS personas (
				uuid TEXT PRIMARY KEY,
				owner_user_id INTEGER NOT NULL,
				owner_name TEXT NOT NULL,
				name TEXT NOT NULL,
				description TEXT,
				avatar_file TEXT,
				synced_at TEXT NOT NULL,
				archived_at TEXT
			)`,
			`CREATE TABLE IF NOT EXISTS workspace_personas (
				persona_uuid TEXT PRIMARY KEY REFERENCES personas(uuid),
				permissions TEXT NOT NULL,
				enabled_at TEXT NOT NULL,
				disabled_at TEXT
			)`,
		],
	},
	{
		version: 8,
		statements: [
			`CREATE TABLE IF NOT EXISTS activity_log (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				at TEXT NOT NULL,
				actor_kind TEXT NOT NULL,
				actor_id TEXT NOT NULL,
				actor_name TEXT,
				method TEXT NOT NULL,
				route TEXT NOT NULL,
				entity TEXT,
				entity_id TEXT,
				status INTEGER NOT NULL
			)`,
			`CREATE INDEX IF NOT EXISTS activity_log_actor_idx ON activity_log (actor_kind, actor_id)`,
		],
	},
	{
		version: 9,
		statements: [
			`CREATE TABLE IF NOT EXISTS pages (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				parent_id INTEGER REFERENCES pages(id) ON DELETE SET NULL,
				title TEXT NOT NULL,
				slug TEXT NOT NULL UNIQUE,
				type TEXT NOT NULL DEFAULT 'plain',
				body TEXT NOT NULL DEFAULT '',
				properties TEXT NOT NULL DEFAULT '{}',
				version INTEGER NOT NULL DEFAULT 1,
				author_id INTEGER NOT NULL,
				author_kind TEXT NOT NULL,
				author_ref TEXT NOT NULL,
				updated_by_id INTEGER NOT NULL,
				updated_by_kind TEXT NOT NULL,
				updated_by_ref TEXT NOT NULL,
				position INTEGER NOT NULL DEFAULT 0,
				pinned INTEGER NOT NULL DEFAULT 0,
				created_at TEXT NOT NULL,
				updated_at TEXT NOT NULL,
				deleted_at TEXT
			)`,
			`CREATE INDEX IF NOT EXISTS pages_parent_idx ON pages (parent_id)`,
			`CREATE TABLE IF NOT EXISTS page_versions (
				page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
				version INTEGER NOT NULL,
				title TEXT NOT NULL,
				body TEXT NOT NULL,
				properties TEXT NOT NULL DEFAULT '{}',
				author_id INTEGER NOT NULL,
				author_kind TEXT NOT NULL,
				author_ref TEXT NOT NULL,
				summary TEXT,
				created_at TEXT NOT NULL,
				PRIMARY KEY (page_id, version)
			)`,
			`CREATE TABLE IF NOT EXISTS page_links (
				page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
				target_kind TEXT NOT NULL,
				target_id INTEGER NOT NULL,
				UNIQUE (page_id, target_kind, target_id)
			)`,
			`CREATE INDEX IF NOT EXISTS page_links_target_idx ON page_links (target_kind, target_id)`,
			`CREATE TABLE IF NOT EXISTS task_page_mentions (
				task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
				page_id INTEGER NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
				PRIMARY KEY (task_id, page_id)
			)`,
			`CREATE INDEX IF NOT EXISTS task_page_mentions_page_idx ON task_page_mentions (page_id)`,
			`CREATE VIRTUAL TABLE IF NOT EXISTS pages_fts USING fts5(title, body, content='pages', content_rowid='id')`,
			`CREATE TRIGGER IF NOT EXISTS pages_fts_ai AFTER INSERT ON pages BEGIN
				INSERT INTO pages_fts(rowid, title, body) VALUES (new.id, new.title, new.body);
			END`,
			`CREATE TRIGGER IF NOT EXISTS pages_fts_ad AFTER DELETE ON pages BEGIN
				INSERT INTO pages_fts(pages_fts, rowid, title, body) VALUES ('delete', old.id, old.title, old.body);
			END`,
			`CREATE TRIGGER IF NOT EXISTS pages_fts_au AFTER UPDATE OF title, body ON pages BEGIN
				INSERT INTO pages_fts(pages_fts, rowid, title, body) VALUES ('delete', old.id, old.title, old.body);
				INSERT INTO pages_fts(rowid, title, body) VALUES (new.id, new.title, new.body);
			END`,
			// files.task_id loses NOT NULL, which needs a table rebuild; ordered so a cut-short run can repeat.
			`CREATE TABLE IF NOT EXISTS files (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				task_id INTEGER REFERENCES tasks(id) ON DELETE CASCADE,
				page_id INTEGER REFERENCES pages(id) ON DELETE CASCADE,
				name TEXT NOT NULL,
				file_path TEXT NOT NULL UNIQUE,
				mime_type TEXT,
				size INTEGER,
				created_at TEXT NOT NULL
			)`,
			`CREATE TABLE IF NOT EXISTS files_v9 (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				task_id INTEGER REFERENCES tasks(id) ON DELETE CASCADE,
				page_id INTEGER REFERENCES pages(id) ON DELETE CASCADE,
				name TEXT NOT NULL,
				file_path TEXT NOT NULL UNIQUE,
				mime_type TEXT,
				size INTEGER,
				created_at TEXT NOT NULL
			)`,
			`INSERT OR IGNORE INTO files_v9 (id, task_id, name, file_path, mime_type, size, created_at)
				SELECT id, task_id, name, file_path, mime_type, size, created_at FROM files`,
			`DROP TABLE IF EXISTS files`,
			`ALTER TABLE files_v9 RENAME TO files`,
			`CREATE INDEX IF NOT EXISTS files_task_idx ON files (task_id)`,
			`CREATE INDEX IF NOT EXISTS files_page_idx ON files (page_id)`,
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
			try {
				await db.execute(statement);
			} catch (error) {
				// A migration cut short may have already added this column; SQLite has no
				// "ADD COLUMN IF NOT EXISTS", so re-running it is expected to hit this once. The
				// error can be a plain string (Tauri's invoke rejects with the Rust side's String)
				// or an Error (node:sqlite in tests), so match on whichever message it carries.
				const message =
					typeof error === 'string'
						? error
						: typeof (error as { message?: unknown })?.message === 'string'
						  ? (error as { message: string }).message
						  : '';
				if (!/duplicate column name/i.test(message)) throw error;
			}
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
