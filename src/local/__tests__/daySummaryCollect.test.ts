import { getActivityFeed } from '@/actions/tmgr/dashboard';
import { collectDay } from '../daySummaryCollect';
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
) => ({
	select: jest.fn(async (sql: string) =>
		sql.startsWith('SELECT task_id') ? comments : tasks,
	),
});

const setLocal = (
	byCode: Record<string, { tasks?: unknown[]; comments?: unknown[] }>,
) => {
	const dbs: Record<string, ReturnType<typeof fakeDb>> = {};
	for (const [code, data] of Object.entries(byCode))
		dbs[code] = fakeDb(data.tasks as any[], data.comments);
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
		const taskQuery = dbs.ws1.select.mock.calls.find(([sql]) =>
			sql.includes('FROM tasks'),
		);
		expect(taskQuery?.[1]).toEqual([FROM, TO, FROM, TO, FROM, TO]);
	});

	it('summarises comments from today, keeping the count, up to three snippets and minutes', async () => {
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
				minutes: 90,
				createdToday: false,
				touchedToday: false,
				comments: 4,
				commentSnippets: ['first', 'second', 'third'],
				source: 'local',
			},
		]);
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
	it('sends the signed-in user id and the local day window on every feed request', async () => {
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
				filters: {
					user_id: 5,
					date_from: '2026-11-06',
					date_to: '2026-11-07',
				},
			});
		}
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
				source: 'cloud',
			},
		]);
	});
});
