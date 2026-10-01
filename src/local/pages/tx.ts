import type { LocalDb } from '../types';

const queues = new WeakMap<LocalDb, Promise<unknown>>();

/** One page write at a time per workspace database: its statements share a single SQLite connection. */
export const exclusive = <T>(db: LocalDb, work: () => Promise<T>): Promise<T> => {
	const previous = queues.get(db) ?? Promise.resolve();
	const run = previous.then(work, work);
	queues.set(
		db,
		run.then(
			() => undefined,
			() => undefined,
		),
	);
	return run;
};

export const inTransaction = async <T>(db: LocalDb, work: () => Promise<T>): Promise<T> => {
	await db.execute('BEGIN IMMEDIATE');
	try {
		const result = await work();
		await db.execute('COMMIT');
		return result;
	} catch (error) {
		try {
			await db.execute('ROLLBACK');
		} catch {
			// the original failure is the one to report
		}
		throw error;
	}
};
