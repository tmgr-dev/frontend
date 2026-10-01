import type { LocalDb } from '../types';

const queues = new WeakMap<LocalDb, Promise<unknown>>();

/** One page write at a time per workspace database within this window; `LocalDb.batch` keeps other windows apart. */
export const exclusive = <T>(
	db: LocalDb,
	work: () => Promise<T>,
): Promise<T> => {
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
