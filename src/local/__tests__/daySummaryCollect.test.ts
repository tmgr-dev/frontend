import { getActivityFeed } from '@/actions/tmgr/dashboard';
import { collectDay, utcDaysOf } from '../daySummaryCollect';
import { listLocalWorkspaces, localContext } from '../runtime';

jest.mock('../runtime', () => ({
	listLocalWorkspaces: jest.fn(),
	localContext: jest.fn(),
}));
jest.mock('@/actions/tmgr/dashboard', () => ({
	getActivityFeed: jest.fn(),
}));

const NOW = new Date(2026, 10, 7, 12);
const FROM = new Date(2026, 10, 7).toISOString();
const TO = new Date(2026, 10, 8).toISOString();
const at = (day: number, hour: number, minute = 0) =>
	new Date(2026, 10, day, hour, minute).toISOString();

const workspace = (id: number, name: string, demo = false) => ({
	id,
	name,
	code: `ws${id}`,
	schema_version: 1,
	created_at: at(1, 0),
	path: '',
	database: '',
	demo,
});

const localTask = (over: Record<string, unknown>) => ({
	id: 1,
	title: 'Local task',
	common_time: 0,
	created_at: at(1, 9),
	updated_at: at(1, 9),
	status: 'In progress',
	category_code: null,
	seq: null,
	...over,
});

const fakeDb = (
	tasks: Record<string, unknown>[],
	comments: unknown[] = [],
	moves: unknown[] = [],
) => ({
	select: jest.fn(async (sql: string) => {
		if (sql.startsWith('SELECT h.task_id')) return moves;
		return sql.startsWith('SELECT task_id') ? comments : tasks;
	}),
});

const setLocal = (
	byCode: Record<
		string,
		{ tasks?: unknown[]; comments?: unknown[]; moves?: unknown[] }
	>,
) => {
	const dbs: Record<string, ReturnType<typeof fakeDb>> = {};
	for (const [code, data] of Object.entries(byCode))
		dbs[code] = fakeDb(data.tasks as any[], data.comments, data.moves);
	(localContext as jest.Mock).mockImplementation(
		async (ws: { code: string }) => ({
			db: dbs[ws.code],
		}),
	);
	return dbs;
};

const feed = (...items: unknown[]) => ({
	success: true,
	data: { data: items, meta: {}, links: {} },
});

const cloudActivity = (over: Record<string, unknown>) => ({
	id: 1,
	type: 'task_created',
	title: '',
	subject_name: 'Ship it',
	description: '',
	user: { id: 5, name: 'Ann', email: '' },
	workspace_id: 2,
	subject_id: 40,
	metadata: {},
	created_at: at(7, 9),
	timestamp_human: '',
	...over,
});

const run = (over: Record<string, unknown> = {}) =>
	collectDay({
		now: NOW,
		localUserId: 5,
		cloud: [],
		cloudUserId: null,
		...over,
	} as Parameters<typeof collectDay>[0]);

beforeEach(() => {
	jest.resetAllMocks();
	(listLocalWorkspaces as jest.Mock).mockResolvedValue([]);
	(getActivityFeed as jest.Mock).mockResolvedValue(feed());
});

describe('collectDay local workspaces', () => {
	it('keeps tasks changed after local midnight and drops those from 23:59 the day before', async () => {
		(listLocalWorkspaces as jest.Mock).mockResolvedValue([
			workspace(1, 'Personal'),
		]);
		const dbs = setLocal({
			ws1: {
				tasks: [
					localTask({
						id: 1,
						title: 'Late yesterday',
						created_at: at(6, 23, 59),
						updated_at: at(6, 23, 59),
					}),
					localTask({
						id: 2,
						title: 'Early today',
						created_at: at(7, 0, 1),
						updated_at: at(7, 0, 1),
					}),
				],
			},
		});

		const { tasks, skipped } = await run();

		expect(tasks.map((t) => t.title)).toEqual(['Early today']);
		expect(tasks[0]).toMatchObject({
			workspace: 'Personal',
			createdToday: true,
			source: 'local',
		});
		expect(skipped).toEqual([]);
		expect(localContext).toHaveBeenCalledWith(
			expect.objectContaining({ id: 1 }),
			expect.objectContaining({ id: 5 }),
		);
		const taskQuery = (dbs.ws1.select.mock.calls as unknown[][]).find(([sql]) =>
			(sql as string).includes('FROM tasks'),
		);
		expect(taskQuery?.[1]).toEqual([FROM, TO, FROM, TO, FROM, TO, FROM, TO]);
	});

	it('summarises comments from today, keeping the count and up to three snippets, without lifetime minutes', async () => {
		(listLocalWorkspaces as jest.Mock).mockResolvedValue([
			workspace(1, 'Personal'),
		]);
		setLocal({
			ws1: {
				tasks: [
					localTask({
						id: 9,
						title: 'Reviewed',
						common_time: 5400,
						created_at: at(3, 9),
						updated_at: at(3, 9),
						category_code: 'TM',
						seq: 12,
					}),
				],
				comments: [
					{ task_id: 9, message: 'yesterday note', created_at: at(6, 20) },
					{
						task_id: 9,
						message: 'from a persona',
						created_at: at(7, 8),
						author_kind: 'persona',
					},
					{ task_id: 9, message: 'first', created_at: at(7, 9) },
					{ task_id: 9, message: ' second ', created_at: at(7, 10) },
					{ task_id: 9, message: 'third', created_at: at(7, 11) },
					{ task_id: 9, message: 'fourth', created_at: at(7, 12) },
				],
			},
		});

		const { tasks } = await run();

		expect(tasks).toEqual([
			{
				workspace: 'Personal',
				key: 'TM-12',
				title: 'Reviewed',
				status: 'In progress',
				createdToday: false,
				touchedToday: false,
				comments: 4,
				commentSnippets: ['first', 'second', 'third'],
				moves: [],
				source: 'local',
			},
		]);
	});

	it('reads a local task created in the day from an offset-less timestamp as UTC', async () => {
		(listLocalWorkspaces as jest.Mock).mockResolvedValue([
			workspace(1, 'Personal'),
		]);
		const utcLocal = (date: Date) => date.toISOString().slice(0, 19);
		setLocal({
			ws1: {
				tasks: [
					localTask({
						id: 3,
						title: 'Naive UTC today',
						created_at: utcLocal(new Date(2026, 10, 7, 9)),
						updated_at: utcLocal(new Date(2026, 10, 7, 9)),
					}),
					localTask({
						id: 4,
						title: 'Naive UTC yesterday',
						created_at: utcLocal(new Date(2026, 10, 6, 23, 59)),
						updated_at: utcLocal(new Date(2026, 10, 6, 23, 59)),
					}),
				],
			},
		});

		const { tasks } = await run();

		expect(tasks.map((t) => t.title)).toEqual(['Naive UTC today']);
	});

	it('never opens demo workspaces', async () => {
		(listLocalWorkspaces as jest.Mock).mockResolvedValue([
			workspace(1, 'Sample', true),
			workspace(2, 'Personal'),
		]);
		setLocal({
			ws1: { tasks: [localTask({ title: 'Demo task', created_at: at(7, 9) })] },
			ws2: { tasks: [localTask({ title: 'Real task', created_at: at(7, 9) })] },
		});

		const { tasks } = await run();

		expect(localContext).toHaveBeenCalledTimes(1);
		expect(tasks.map((t) => t.title)).toEqual(['Real task']);
	});

	it('reports a local workspace whose query fails as skipped', async () => {
		(listLocalWorkspaces as jest.Mock).mockResolvedValue([
			workspace(1, 'Broken'),
			workspace(2, 'Personal'),
		]);
		setLocal({
			ws2: { tasks: [localTask({ title: 'Works', created_at: at(7, 9) })] },
		});
		(localContext as jest.Mock).mockImplementation(
			async (ws: { code: string }) => {
				if (ws.code === 'ws1') throw new Error('database locked');
				return {
					db: fakeDb([localTask({ title: 'Works', created_at: at(7, 9) })]),
				};
			},
		);

		const { tasks, skipped } = await run();

		expect(skipped).toEqual(['Broken']);
		expect(tasks.map((t) => t.title)).toEqual(['Works']);
	});
});

describe('collectDay cloud workspaces', () => {
	it('sends the signed-in user id and the UTC day on every feed request', async () => {
		(listLocalWorkspaces as jest.Mock).mockResolvedValue([]);

		await run({
			cloud: [
				{ id: 2, name: 'Work' },
				{ id: 3, name: 'Team' },
			],
			cloudUserId: 5,
		});

		expect(getActivityFeed).toHaveBeenCalledTimes(2);
		const calls = (getActivityFeed as jest.Mock).mock.calls;
		expect(calls.map(([id]) => id)).toEqual([2, 3]);
		for (const [, params] of calls) {
			expect(params).toMatchObject({
				per_page: 100,
				filters: { user_id: 5, date: '2026-11-07' },
			});
			expect(params.filters).not.toHaveProperty('date_from');
			expect(params.filters).not.toHaveProperty('date_to');
		}
	});

	it('drops activities performed by a teammate', async () => {
		(getActivityFeed as jest.Mock).mockResolvedValue(
			feed(
				cloudActivity({ id: 1, subject_id: 40, subject_name: 'Mine' }),
				cloudActivity({
					id: 2,
					subject_id: 41,
					subject_name: 'Teammate',
					user: { id: 6, name: 'Bob', email: '' },
				}),
			),
		);

		const { tasks } = await run({
			cloud: [{ id: 2, name: 'Ops' }],
			cloudUserId: 5,
		});

		expect(tasks.map((t) => t.title)).toEqual(['Mine']);
	});

	it('takes the task of a comment from metadata and merges it into the same task', async () => {
		(getActivityFeed as jest.Mock).mockResolvedValue(
			feed(
				cloudActivity({
					id: 2,
					type: 'comment_created',
					subject_type: 'comment',
					subject_id: 900,
					subject_name: '',
					created_at: at(7, 10),
					metadata: { comment: 'on the task', task_id: 40 },
				}),
				cloudActivity({ id: 1, subject_id: 40, created_at: at(7, 9) }),
				cloudActivity({
					id: 3,
					type: 'comment_created',
					subject_type: 'comment',
					subject_id: 901,
					subject_name: '',
					created_at: at(7, 11),
					metadata: { comment: 'orphan' },
				}),
			),
		);

		const { tasks } = await run({
			cloud: [{ id: 2, name: 'Ops' }],
			cloudUserId: 5,
		});

		expect(tasks).toEqual([
			expect.objectContaining({
				key: '#40',
				title: 'Ship it',
				comments: 1,
				commentSnippets: ['on the task'],
			}),
		]);
	});

	it('loads the next feed page while the page is full and the day has not been left', async () => {
		const full = (start: number, created: string) =>
			feed(
				...Array.from({ length: 100 }, (_, i) =>
					cloudActivity({
						id: start + i,
						subject_id: start + i,
						created_at: created,
					}),
				),
			);
		(getActivityFeed as jest.Mock).mockImplementation(
			async (_id: number, params: { page: number }) => {
				if (params.page === 1) return full(1000, at(7, 9));
				return feed(
					cloudActivity({ id: 9000, subject_id: 9000, created_at: at(7, 8) }),
				);
			},
		);

		const { tasks } = await run({
			cloud: [{ id: 2, name: 'Ops' }],
			cloudUserId: 5,
		});

		expect(
			(getActivityFeed as jest.Mock).mock.calls.map(
				([, params]) => params.page,
			),
		).toEqual([1, 2]);
		expect(tasks.some((t) => t.key === '#9000')).toBe(true);
		expect(tasks).toHaveLength(101);
	});

	it('stops paging once a page reaches the day start', async () => {
		(getActivityFeed as jest.Mock).mockImplementation(async () =>
			feed(
				...Array.from({ length: 99 }, (_, i) =>
					cloudActivity({ id: i, subject_id: i + 1, created_at: at(7, 9) }),
				),
				cloudActivity({ id: 500, subject_id: 500, created_at: at(6, 23) }),
			),
		);

		await run({ cloud: [{ id: 2, name: 'Ops' }], cloudUserId: 5 });

		expect(getActivityFeed).toHaveBeenCalledTimes(1);
	});

	it('caps the cloud feed at five pages', async () => {
		(getActivityFeed as jest.Mock).mockImplementation(async () =>
			feed(
				...Array.from({ length: 100 }, (_, i) =>
					cloudActivity({ id: i, subject_id: i + 1, created_at: at(7, 9) }),
				),
			),
		);

		const { skipped } = await run({
			cloud: [{ id: 2, name: 'Ops' }],
			cloudUserId: 5,
		});

		expect(getActivityFeed).toHaveBeenCalledTimes(5);
		expect(skipped).toEqual(['Ops (partial)']);
	});

	it('reads an offset-less cloud timestamp as UTC at the day boundary', async () => {
		const utcLocal = (date: Date) => date.toISOString().slice(0, 19);
		(getActivityFeed as jest.Mock).mockResolvedValue(
			feed(
				cloudActivity({
					id: 1,
					subject_id: 40,
					subject_name: 'Today',
					created_at: utcLocal(new Date(2026, 10, 7, 0, 30)),
				}),
				cloudActivity({
					id: 2,
					subject_id: 41,
					subject_name: 'Yesterday',
					created_at: utcLocal(new Date(2026, 10, 6, 23, 30)),
				}),
			),
		);

		const { tasks } = await run({
			cloud: [{ id: 2, name: 'Ops' }],
			cloudUserId: 5,
		});

		expect(tasks.map((t) => t.title)).toEqual(['Today']);
	});

	it('reports failing cloud workspaces as skipped while the others still return', async () => {
		(getActivityFeed as jest.Mock).mockImplementation(async (id: number) => {
			if (id === 2) throw new Error('network down');
			if (id === 3) return { success: false, error: { message: 'forbidden' } };
			return feed(cloudActivity({ subject_name: 'Ship it' }));
		});

		const { tasks, skipped } = await run({
			cloud: [
				{ id: 2, name: 'Work' },
				{ id: 3, name: 'Team' },
				{ id: 4, name: 'Ops' },
			],
			cloudUserId: 5,
		});

		expect(skipped.sort()).toEqual(['Team', 'Work']);
		expect(tasks.map((t) => [t.workspace, t.title])).toEqual([
			['Ops', 'Ship it'],
		]);
	});

	it('skips the cloud entirely when there is no cloud user', async () => {
		(listLocalWorkspaces as jest.Mock).mockResolvedValue([
			workspace(1, 'Personal'),
		]);
		setLocal({
			ws1: { tasks: [localTask({ title: 'Local', created_at: at(7, 9) })] },
		});

		const { tasks, skipped } = await run({
			cloud: [{ id: 2, name: 'Work' }],
			cloudUserId: null,
		});

		expect(getActivityFeed).not.toHaveBeenCalled();
		expect(tasks.map((t) => t.title)).toEqual(['Local']);
		expect(skipped).toEqual([]);
	});

	it('maps activities by subject, taking the newest status and the time and comments', async () => {
		(getActivityFeed as jest.Mock).mockResolvedValue(
			feed(
				cloudActivity({
					id: 10,
					type: 'task_status_changed',
					created_at: at(7, 10),
					metadata: JSON.stringify({ new_status: 3, new_status_name: 'Done' }),
				}),
				cloudActivity({
					id: 9,
					type: 'task_status_changed',
					created_at: at(7, 9),
					metadata: { new_status: 2, new_status_name: 'Doing' },
				}),
				cloudActivity({
					id: 8,
					type: 'comment_created',
					created_at: at(7, 9, 30),
					metadata: { comment: 'looks good' },
				}),
				cloudActivity({
					id: 7,
					type: 'task_timer_stopped',
					created_at: at(7, 9, 45),
					metadata: JSON.stringify({ time_added: 1800 }),
				}),
				cloudActivity({
					id: 6,
					type: 'task_created',
					subject_id: 41,
					subject_name: '',
					created_at: at(7, 8),
					metadata: { task_title: 'From metadata' },
				}),
				cloudActivity({
					id: 5,
					type: 'task_created',
					subject_id: 42,
					created_at: at(6, 23, 59),
				}),
				cloudActivity({ id: 4, type: 'task_updated', subject_id: 43 }),
			),
		);

		const { tasks } = await run({
			cloud: [{ id: 2, name: 'Ops' }],
			cloudUserId: 5,
		});

		expect(tasks).toEqual([
			{
				workspace: 'Ops',
				key: '#40',
				title: 'Ship it',
				status: 'Done',
				minutes: 30,
				createdToday: false,
				touchedToday: true,
				comments: 1,
				commentSnippets: ['looks good'],
				moves: [],
				source: 'cloud',
			},
			{
				workspace: 'Ops',
				key: '#41',
				title: 'From metadata',
				status: '',
				createdToday: true,
				touchedToday: true,
				comments: 0,
				commentSnippets: [],
				moves: [],
				source: 'cloud',
			},
		]);
	});
});

describe('collectDay status moves', () => {
	const moveRow = (over: Record<string, unknown>) => ({
		task_id: 1,
		created_at: at(7, 9),
		from_name: 'Backlog',
		to_name: 'In progress',
		actor_kind: 'user',
		...over,
	});

	it('returns local moves in chronological order, including a task only moved today', async () => {
		(listLocalWorkspaces as jest.Mock).mockResolvedValue([
			workspace(1, 'Personal'),
		]);
		const dbs = setLocal({
			ws1: {
				tasks: [
					localTask({
						id: 1,
						title: 'Old task',
						created_at: at(1, 9),
						updated_at: at(1, 9),
						status: 'Done',
					}),
				],
				moves: [
					moveRow({ created_at: at(6, 23, 59), from_name: 'X', to_name: 'Y' }),
					moveRow({ created_at: at(7, 9) }),
					moveRow({
						created_at: at(7, 11),
						from_name: 'In progress',
						to_name: 'Done',
					}),
					moveRow({ created_at: at(7, 12), actor_kind: 'persona' }),
					moveRow({ created_at: at(7, 13), from_name: null }),
				],
			},
		});

		const { tasks } = await run();

		expect(tasks).toHaveLength(1);
		expect(tasks[0].moves).toEqual([
			{ from: 'Backlog', to: 'In progress' },
			{ from: 'In progress', to: 'Done' },
		]);
		const movesQuery = (dbs.ws1.select.mock.calls as unknown[][]).find(
			([sql]) => (sql as string).includes('task_status_history'),
		);
		expect(movesQuery?.[1]).toEqual([FROM, TO]);
	});

	it('adds a move from the activity row names', async () => {
		(getActivityFeed as jest.Mock).mockResolvedValue(
			feed(
				cloudActivity({
					id: 2,
					type: 'task_status_changed',
					created_at: at(7, 11),
					old_status_name: 'Doing',
					new_status_name: 'Done',
				}),
				cloudActivity({
					id: 1,
					type: 'task_status_changed',
					created_at: at(7, 9),
					old_status_name: 'Backlog',
					new_status_name: 'Doing',
				}),
			),
		);

		const { tasks } = await run({
			cloud: [{ id: 2, name: 'Ops' }],
			cloudUserId: 5,
		});

		expect(tasks[0].moves).toEqual([
			{ from: 'Backlog', to: 'Doing' },
			{ from: 'Doing', to: 'Done' },
		]);
		expect(tasks[0].status).toBe('Done');
	});

	it('keeps the newest status even when its name is gone', async () => {
		(getActivityFeed as jest.Mock).mockResolvedValue(
			feed(
				cloudActivity({
					id: 2,
					type: 'task_status_changed',
					created_at: at(7, 11),
					old_status_name: 'Doing',
					new_status_name: null,
				}),
				cloudActivity({
					id: 1,
					type: 'task_status_changed',
					created_at: at(7, 9),
					old_status_name: 'Backlog',
					new_status_name: 'Doing',
				}),
			),
		);

		const { tasks } = await run({
			cloud: [{ id: 2, name: 'Ops' }],
			cloudUserId: 5,
		});

		expect(tasks[0].status).toBe('');
		expect(tasks[0].moves).toEqual([{ from: 'Backlog', to: 'Doing' }]);
	});

	it('adds no move when the names are missing', async () => {
		(getActivityFeed as jest.Mock).mockResolvedValue(
			feed(
				cloudActivity({
					type: 'task_status_changed',
					created_at: at(7, 10),
					metadata: { new_status: 3, new_status_name: 'Done' },
				}),
				cloudActivity({
					id: 2,
					type: 'task_status_changed',
					old_status_name: 'Backlog',
				}),
			),
		);

		const { tasks } = await run({
			cloud: [{ id: 2, name: 'Ops' }],
			cloudUserId: 5,
		});

		expect(tasks[0].moves).toEqual([]);
		expect(tasks[0].status).toBe('Done');
	});

	it('lists every UTC day a local day overlaps', () => {
		expect(
			utcDaysOf({
				from: new Date('2026-11-06T21:00:00Z'),
				to: new Date('2026-11-07T21:00:00Z'),
			}),
		).toEqual(['2026-11-06', '2026-11-07']);
		expect(
			utcDaysOf({
				from: new Date('2026-11-07T05:00:00Z'),
				to: new Date('2026-11-08T05:00:00Z'),
			}),
		).toEqual(['2026-11-07', '2026-11-08']);
		expect(
			utcDaysOf({
				from: new Date('2026-11-07T00:00:00Z'),
				to: new Date('2026-11-08T00:00:00Z'),
			}),
		).toEqual(['2026-11-07']);
	});
});
