import { boardTaskCounts } from '../boardSummary';

const column = (type: string, count?: number, tasks: unknown[] = []) => ({
	status: { type },
	taskCount: count,
	tasks,
});

describe('boardTaskCounts', () => {
	it('adds up the tasks of every column', () => {
		const counts = boardTaskCounts([
			column('default', 3),
			column('active', 2),
			column('completed', 5),
		]);
		expect(counts.total).toBe(10);
		expect(counts.inProgress).toBe(2);
		expect(counts.done).toBe(5);
	});

	it('leaves the archive out of the board total', () => {
		const counts = boardTaskCounts([column('default', 4), column('archived', 900)]);
		expect(counts.total).toBe(4);
	});

	it('counts hidden columns separately but inside the total', () => {
		const counts = boardTaskCounts([column('default', 1), column('hidden', 2)]);
		expect(counts).toMatchObject({ total: 3, hidden: 2 });
	});

	it('falls back to the loaded tasks when a column carries no count', () => {
		const counts = boardTaskCounts([column('default', undefined, [{}, {}])]);
		expect(counts.total).toBe(2);
	});

	it('reports the share of finished tasks', () => {
		expect(boardTaskCounts([column('default', 1), column('completed', 3)]).percent).toBe(75);
	});

	it('reports no progress for an empty board instead of dividing by zero', () => {
		expect(boardTaskCounts([])).toEqual({
			total: 0,
			inProgress: 0,
			done: 0,
			hidden: 0,
			percent: 0,
		});
	});

	it('survives a column that has neither a status nor tasks', () => {
		expect(boardTaskCounts([{}, null, undefined]).total).toBe(0);
	});
});
