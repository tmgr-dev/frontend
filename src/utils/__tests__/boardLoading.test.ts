import {
	createBoardLoader,
	filterBoardTasks,
	MY_PERSONAS_FILTER,
} from '../boardLoading';

const flush = async () => {
	await Promise.resolve();
	await Promise.resolve();
};

describe('board loading', () => {
	beforeEach(() => jest.useFakeTimers());
	afterEach(() => jest.useRealTimers());
	it('coalesces rapid changes and applies only the latest result', async () => {
		const resolvers: Array<(value: string) => void> = [];
		const fetch = jest.fn(
			() => new Promise<string>((resolve) => resolvers.push(resolve)),
		);
		const apply = jest.fn();
		const loader = createBoardLoader(fetch, apply, jest.fn(), jest.fn());
		void loader.run();
		loader.schedule();
		loader.schedule();
		resolvers[0]('old');
		await flush();
		expect(apply).not.toHaveBeenCalled();
		jest.advanceTimersByTime(300);
		expect(fetch).toHaveBeenCalledTimes(2);
		resolvers[1]('new');
		await flush();
		expect(apply).toHaveBeenCalledWith('new');
	});
	it('reports failure and allows retry without applying failed data', async () => {
		const apply = jest.fn();
		const error = jest.fn();
		const busy = jest.fn();
		const fetch = jest
			.fn()
			.mockRejectedValueOnce(new Error('offline'))
			.mockResolvedValueOnce('ok');
		const loader = createBoardLoader(fetch, apply, error, busy);
		await loader.run();
		expect(error).toHaveBeenCalledWith(expect.any(Error));
		expect(apply).not.toHaveBeenCalled();
		expect(busy).toHaveBeenLastCalledWith(false);
		await loader.run();
		expect(apply).toHaveBeenCalledWith('ok');
	});
	it('cancels timers and ignores responses after disposal', async () => {
		let resolve!: (value: string) => void;
		const fetch = jest.fn(
			() =>
				new Promise<string>((r) => {
					resolve = r;
				}),
		);
		const apply = jest.fn();
		const loader = createBoardLoader(fetch, apply, jest.fn(), jest.fn());
		void loader.run();
		loader.schedule();
		loader.dispose();
		resolve('late');
		await flush();
		jest.runAllTimers();
		expect(fetch).toHaveBeenCalledTimes(1);
		expect(apply).not.toHaveBeenCalled();
	});
	it('combines user and category filters', () => {
		const tasks = [
			{ project_category_id: 2, assignees: [{ id: 3 }] },
			{ project_category_id: 2 },
			{ assignees: [{ id: 3 }] },
		];
		expect(
			filterBoardTasks(tasks, { selectedUser: 3, selectedCategory: 2 }),
		).toEqual([tasks[0]]);
		expect(filterBoardTasks(tasks, {})).toEqual(tasks);
	});
	it('filters by a persona uuid and by "my personas" queue', () => {
		const persona = (id: string, ownerId: number) => ({
			id,
			name: id,
			avatar_url: null,
			owner: { id: ownerId, name: 'Owner' },
			workspace_id: null,
		});
		const tasks = [
			{ title: 'A', persona_assignees: [persona('p1', 1)] },
			{ title: 'B', persona_assignees: [persona('p2', 9)] },
			{ title: 'C' },
		];
		expect(filterBoardTasks(tasks, { selectedPersona: 'p2' })).toEqual([
			tasks[1],
		]);
		expect(
			filterBoardTasks(tasks, {
				selectedPersona: MY_PERSONAS_FILTER,
				myUserId: 1,
			}),
		).toEqual([tasks[0]]);
		expect(filterBoardTasks(tasks, { selectedPersona: '' })).toEqual(tasks);
	});
	it('filters by priority levels and sorts by priority after filtering', () => {
		const tasks = [
			{ id: 1, priority: 'low', project_category_id: 2 },
			{ id: 2, project_category_id: 2 },
			{ id: 3, priority: 'urgent', project_category_id: 2 },
			{ id: 4, priority: 'high', project_category_id: 5 },
			{ id: 5, priority: 'urgent', project_category_id: 2 },
		];
		const ids = (list: Array<{ id: number }>) => list.map((task) => task.id);
		expect(
			ids(filterBoardTasks(tasks, { selectedPriorities: ['urgent', 'high'] })),
		).toEqual([3, 4, 5]);
		expect(
			ids(filterBoardTasks(tasks, { selectedPriorities: ['medium'] })),
		).toEqual([2]);
		expect(ids(filterBoardTasks(tasks, { prioritySort: true }))).toEqual([
			3, 5, 4, 2, 1,
		]);
		expect(
			ids(
				filterBoardTasks(tasks, {
					selectedCategory: 2,
					selectedPriorities: ['urgent', 'low'],
					prioritySort: true,
				}),
			),
		).toEqual([3, 5, 1]);
		expect(ids(filterBoardTasks(tasks, { selectedPriorities: [] }))).toEqual([
			1, 2, 3, 4, 5,
		]);
	});
});
