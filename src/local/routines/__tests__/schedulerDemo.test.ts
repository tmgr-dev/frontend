import type { LocalContext, LocalWorkspace } from '../../types';
import type { RoutineEntryJson } from '../service';

const materializeDueInstances = jest.fn().mockResolvedValue(0);
const expandRange = jest.fn();
jest.mock('../service', () => ({
	materializeDueInstances: (...args: unknown[]) =>
		materializeDueInstances(...args),
	expandRange: (...args: unknown[]) => expandRange(...args),
}));

import { runSchedulerTick } from '../scheduler';

const entry = (): RoutineEntryJson => ({
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
});

const base: LocalWorkspace = {
	id: -1,
	name: 'Personal',
	code: 'personal',
	schema_version: 4,
	created_at: '',
	path: '/tmp/personal',
	database: '/tmp/personal/workspace.db',
};

const storage = () => {
	const map = new Map<string, string>();
	return {
		getItem: (key: string) => map.get(key) ?? null,
		setItem: (key: string, value: string) => void map.set(key, value),
	};
};

describe('runSchedulerTick and demo workspaces', () => {
	const ctx = {} as LocalContext;

	beforeEach(() => {
		materializeDueInstances.mockClear().mockResolvedValue(0);
		expandRange.mockClear().mockResolvedValue([entry()]);
	});

	it('does not open or notify for a demo workspace', async () => {
		const context = jest.fn().mockResolvedValue(ctx);
		const notify = jest.fn();

		await runSchedulerTick({
			now: () => new Date('2026-09-27T08:55:00'),
			notify,
			storage: storage(),
			listWorkspaces: async () => [
				{ ...base, code: 'demo', name: 'Demo: Launch week', demo: true },
			],
			context,
		});

		expect(context).not.toHaveBeenCalled();
		expect(materializeDueInstances).not.toHaveBeenCalled();
		expect(notify).not.toHaveBeenCalled();
	});

	it('opens and notifies for a normal workspace', async () => {
		const context = jest.fn().mockResolvedValue(ctx);
		const notify = jest.fn();

		await runSchedulerTick({
			now: () => new Date('2026-09-27T08:55:00'),
			notify,
			storage: storage(),
			listWorkspaces: async () => [{ ...base, demo: false }],
			context,
		});

		expect(context).toHaveBeenCalledTimes(1);
		expect(notify).toHaveBeenCalledTimes(1);
	});
});
