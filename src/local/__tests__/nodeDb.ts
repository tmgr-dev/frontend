import type { LocalDb } from '../types';

/** node:sqlite (Node ≥ 22) behind the LocalDb interface, so the local API runs on real SQLite in jest. */
export const nodeSqliteAvailable = (() => {
	try {
		require('node:sqlite');
		return true;
	} catch {
		return false;
	}
})();

export const memoryDb = (): LocalDb => {
	// eslint-disable-next-line @typescript-eslint/no-var-requires
	const { DatabaseSync } = require('node:sqlite');
	const db = new DatabaseSync(':memory:');
	db.exec('PRAGMA foreign_keys = ON');
	return {
		async select(sql, params = []) {
			return db.prepare(sql).all(...params);
		},
		async execute(sql, params = []) {
			const result = db.prepare(sql).run(...params);
			return {
				rowsAffected: Number(result.changes),
				lastInsertId: Number(result.lastInsertRowid),
			};
		},
		async batch(statements) {
			const results: { rowsAffected: number; lastInsertId?: number }[] = [];
			db.exec('BEGIN IMMEDIATE');
			try {
				for (const [index, statement] of statements.entries()) {
					const result = db
						.prepare(statement.sql)
						.run(...(statement.params ?? []));
					results.push({
						rowsAffected: Number(result.changes),
						lastInsertId: Number(result.lastInsertRowid),
					});
					if (statement.expectChanges && !result.changes) {
						db.exec('ROLLBACK');
						return { results: [], failedAt: index };
					}
				}
				db.exec('COMMIT');
				return { results, failedAt: null };
			} catch (error) {
				db.exec('ROLLBACK');
				throw error;
			}
		},
	};
};
