import type { LocalContext, LocalWorkspace } from '../../types';
import type { RoutineEntryJson } from '../service';

const materializeDueInstances = jest.fn().mockResolvedValue(0);
const expandRange = jest.fn().mockResolvedValue([]);
jest.mock('../service', () => ({
	materializeDueInstances: (...args: unknown[]) => materializeDueInstances(...args),
	expandRange: (...args: unknown[]) => expandRange(...args),
}));

import { dueReminders, pruneReminded, reminderKey, runSchedulerTick } from '../scheduler';

const entry = (overrides: Partial<RoutineEntryJson> = {}): RoutineEntryJson => ({
	task_id: 1,
	title: 'Water plants',
	description: null,
	routine_category: { id: 'home', name: 'Home', color: '#000' },
	date: '2026-09-27',
	time: '09:00',
	scheduled_for: null,
	duration_min: null,
	reminder_min: 10,
	frequency: null,
	status: 'PENDING',
	completed: false,
	instance_id: 1,
	virtual: false,
	created_at: null,
	updated_at: null,
	...overrides,
});

describe('dueReminders', () => {
	it('fires once now is inside the reminder window', () => {
		const now = new Date('2026-09-27T08:55:00');
		expect(dueReminders([entry()], now)).toHaveLength(1);
	});

	it('does not fire before the reminder window opens', () => {
		const now = new Date('2026-09-27T08:49:59');
		expect(dueReminders([entry()], now)).toHaveLength(0);
	});

	it('does not fire once the routine time has passed', () => {
		const now = new Date('2026-09-27T09:00:00');
		expect(dueReminders([entry()], now)).toHaveLength(0);
	});

	it('skips entries without a reminder_min', () => {
		const now = new Date('2026-09-27T08:55:00');
		expect(dueReminders([entry({ reminder_min: null })], now)).toHaveLength(0);
	});

	it('skips entries without a time', () => {
		const now = new Date('2026-09-27T08:55:00');
		expect(dueReminders([entry({ time: null })], now)).toHaveLength(0);
	});

	it('skips entries that are not pending', () => {
		const now = new Date('2026-09-27T08:55:00');
		expect(dueReminders([entry({ status: 'COMPLETED' })], now)).toHaveLength(0);
		expect(dueReminders([entry({ status: 'pending' })], now)).toHaveLength(1);
	});
});

describe('pruneReminded', () => {
	it('drops keys older than today and keeps the rest', () => {
		const pruned = pruneReminded(
			{ 'a:1:2026-09-26:09:00': '2026-09-26', 'a:1:2026-09-27:09:00': '2026-09-27' },
			'2026-09-27',
		);
		expect(pruned).toEqual({ 'a:1:2026-09-27:09:00': '2026-09-27' });
	});
});

describe('reminderKey', () => {
	it('scopes the key to workspace, task, date and time', () => {
		expect(reminderKey('personal', entry())).toBe('personal:1:2026-09-27:09:00');
	});
});

describe('runSchedulerTick', () => {
	const workspace: LocalWorkspace = {
		id: -1,
		name: 'Personal',
		code: 'personal',
		schema_version: 4,
		created_at: '',
		path: '/tmp/personal',
		database: '/tmp/personal/workspace.db',
	};
	const ctx = {} as LocalContext;

	const makeStorage = () => {
		const map = new Map<string, string>();
		return {
			getItem: (key: string) => map.get(key) ?? null,
			setItem: (key: string, value: string) => void map.set(key, value),
		};
	};

	beforeEach(() => {
		materializeDueInstances.mockClear().mockResolvedValue(0);
		expandRange.mockClear().mockResolvedValue([]);
	});

	it('materializes due instances, then notifies once for each due entry', async () => {
		expandRange.mockResolvedValue([entry(), entry({ task_id: 2, reminder_min: null })]);
		const notify = jest.fn();
		const context = jest.fn().mockResolvedValue(ctx);
		const now = () => new Date('2026-09-27T08:55:00');

		await runSchedulerTick({
			now,
			notify,
			storage: makeStorage(),
			listWorkspaces: async () => [workspace],
			context,
		});

		expect(context).toHaveBeenCalledWith(workspace);
		expect(materializeDueInstances).toHaveBeenCalledWith(ctx);
		expect(expandRange).toHaveBeenCalledWith(ctx, '2026-09-27', '2026-09-28');
		expect(notify).toHaveBeenCalledTimes(1);
		expect(notify).toHaveBeenCalledWith('Routine: Water plants', 'at 09:00 · Personal (local)');
	});

	it('does not re-notify an already-reminded entry on the next tick', async () => {
		expandRange.mockResolvedValue([entry()]);
		const notify = jest.fn();
		const storage = makeStorage();
		const now = () => new Date('2026-09-27T08:55:00');
		const deps = {
			now,
			notify,
			storage,
			listWorkspaces: async () => [workspace],
			context: async () => ctx,
		};

		await runSchedulerTick(deps);
		await runSchedulerTick(deps);

		expect(notify).toHaveBeenCalledTimes(1);
	});

	it('keeps going when a workspace fails', async () => {
		const other: LocalWorkspace = { ...workspace, id: -2, code: 'work' };
		const context = jest.fn(async (w: LocalWorkspace) => {
			if (w.code === 'personal') throw new Error('db locked');
			return ctx;
		});
		expandRange.mockResolvedValue([entry()]);
		const notify = jest.fn();

		await runSchedulerTick({
			now: () => new Date('2026-09-27T08:55:00'),
			notify,
			storage: makeStorage(),
			listWorkspaces: async () => [workspace, other],
			context,
		});

		expect(notify).toHaveBeenCalledTimes(1);
		expect(notify).toHaveBeenCalledWith('Routine: Water plants', 'at 09:00 · Personal (local)');
	});
});
