import type { LocalWorkspace } from '@/local/types';
import { createDomainEvents, installDomainEvents, type DomainEvent } from '@/utils/domainEvents';
import { memoryDb, nodeSqliteAvailable } from '../../local/__tests__/nodeDb';

const LOCAL: LocalWorkspace = {
	id: -42,
	name: 'Personal',
	code: 'personal',
	schema_version: 1,
	created_at: '',
	path: '/tmp/personal',
	database: '/tmp/personal/workspace.db',
};

const db = nodeSqliteAvailable ? memoryDb() : null;
let clock = new Date('2026-09-26T10:00:00');

jest.mock('@/local/runtime', () => ({
	localWorkspaceById: async (id: number) => (id === -42 ? LOCAL : null),
	localContext: async (workspace: any, user: any, actor: any) => ({
		db,
		workspace,
		user,
		actor,
		now: () => clock,
		files: { url: (key: string) => key, read: async () => new Blob([]), remove: async () => {} },
	}),
}));

// eslint-disable-next-line import/first
import { migrate } from '@/local/schema';
// eslint-disable-next-line import/first
import { dispatchLocal } from '@/local/dispatch';
// eslint-disable-next-line import/first
import { createLocalApi } from '@/local/api';
// eslint-disable-next-line import/first
import { pinnedLocalClient } from '@/local/pinned';
// eslint-disable-next-line import/first
import { createDataApi } from '../dataApi';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('routines through the local router via pinnedLocalClient/createDataApi', () => {
	const rawApi = createLocalApi();
	const seedCtx = () => ({
		db: db!,
		workspace: LOCAL,
		user: { id: 7, name: 'Yurij', email: 'me@example.com' },
		now: () => clock,
		files: { url: (key: string) => key, read: async () => new Blob([]), remove: async () => {} },
	});
	/** Seeds fixtures directly through the raw local router: the plugin API has no way to create a recurrence pattern. */
	const seed = async (method: string, url: string, body?: unknown) => {
		const res = await dispatchLocal(rawApi, seedCtx(), method, url, body);
		if (!res) throw new Error(`no local route for ${method} ${url}`);
		return res.data.data;
	};

	const dataApi = (pluginId = 'tmgr.notes') =>
		createDataApi(
			pinnedLocalClient(LOCAL.id, () => ({ id: 7, name: 'Yurij', email: 'me@example.com' })),
			pluginId,
			pluginId,
			'Notes',
			false,
			LOCAL.id,
		);

	beforeAll(async () => {
		await migrate(db!, clock.toISOString());
	});

	beforeEach(() => {
		clock = new Date('2026-09-26T10:00:00');
	});

	it('lists a period including a recurring routine and an undated note', async () => {
		const recurring = await seed('POST', 'daily-routines/tasks/recurring', {
			title: 'Standup',
			recurrence: { frequency: 'DAILY', interval: 1, time: { hours: 9, minutes: 0 } },
			scheduled_date: '2026-09-26',
			scheduled_time: { hours: 9, minutes: 0 },
		});
		const note = await dataApi().createRoutine({
			title: 'Someday note',
			description: null,
			date: null,
			time: null,
		});

		const entries = await dataApi().listRoutines('2026-09-26', '2026-09-27');
		const standup = entries.find((e) => e.routineId === recurring.id && e.date === '2026-09-26');
		expect(standup).toMatchObject({
			title: 'Standup',
			recurring: true,
			frequency: 'DAILY',
			time: '09:00',
			virtual: false,
		});
		const tomorrow = entries.find((e) => e.routineId === recurring.id && e.date === '2026-09-27');
		expect(tomorrow).toMatchObject({ recurring: true, virtual: true, status: 'PENDING' });

		const noteEntry = entries.find((e) => e.routineId === note.id);
		expect(noteEntry).toMatchObject({
			title: 'Someday note',
			date: '2026-09-26',
			recurring: false,
			frequency: null,
			virtual: true,
		});
	});

	it('creates a one-off routine with a date and reads it back', async () => {
		const created = await dataApi().createRoutine({
			title: 'Dentist',
			description: 'Bring insurance card',
			date: '2026-10-01',
			time: '14:30',
		});
		// The date lives on the created instance, not on the routine's own (unused) scheduled_date column.
		expect(created).toMatchObject({
			title: 'Dentist',
			description: 'Bring insurance card',
		});
		const fetched = await dataApi().getRoutine(created.id);
		expect(fetched).toMatchObject({ id: created.id, title: 'Dentist' });
		const instances = await dataApi().listRoutineInstances(created.id);
		expect(instances).toEqual([
			{ id: expect.any(Number), routineId: created.id, date: '2026-10-01', time: '14:30', status: 'PENDING' },
		]);
	});

	it('renames a routine', async () => {
		const api = dataApi();
		const created = await api.createRoutine({ title: 'Draft', description: null, date: null, time: null });
		const renamed = await api.updateRoutine(created.id, { title: 'Final title' });
		expect(renamed.title).toBe('Final title');
		expect((await api.getRoutine(created.id)).title).toBe('Final title');
	});

	it('completing twice stays COMPLETED (idempotent, not a toggle)', async () => {
		const api = dataApi();
		const created = await api.createRoutine({ title: 'Meditate', description: null, date: null, time: null });
		const first = await api.completeRoutine(created.id, '2026-09-26');
		expect(first.status).toBe('COMPLETED');
		const second = await api.completeRoutine(created.id, '2026-09-26');
		expect(second).toMatchObject({ status: 'COMPLETED', routineId: created.id, date: '2026-09-26' });

		const other = await api.createRoutine({ title: 'Meds', description: null, date: '2026-09-26', time: null });
		const [instance] = await api.listRoutineInstances(other.id);
		await api.completeRoutine(other.id, instance.date);
		const stillCompleted = await api.completeRoutine(other.id, instance.date);
		expect(stillCompleted.status).toBe('COMPLETED');
	});

	it('skips a routine, materializing a virtual date and leaving an existing one alone', async () => {
		const api = dataApi();
		const created = await api.createRoutine({ title: 'Run', description: null, date: null, time: null });
		const skipped = await api.skipRoutine(created.id, '2026-10-05');
		expect(skipped).toMatchObject({ routineId: created.id, date: '2026-10-05', status: 'SKIPPED' });
		const again = await api.skipRoutine(created.id, '2026-10-05');
		expect(again.status).toBe('SKIPPED');
		expect(await api.listRoutineInstances(created.id)).toHaveLength(1);
	});

	it('converts a routine into a task with a category ticket key, and the routine is gone', async () => {
		const api = dataApi();
		const category = await api.createCategory({ title: 'Habits', code: 'HB' });
		const created = await api.createRoutine({ title: 'Read', description: null, date: null, time: null });

		const task = await api.convertRoutine(created.id, {
			categoryId: (category as any).id,
			statusId: null,
		});
		expect(task).toMatchObject({ title: 'Read', key: 'HB-1' });
		await expect(api.getRoutine(created.id)).rejects.toMatchObject({ response: { status: 404 } });
	});

	it('emits routine domain events with the plugin as actor', async () => {
		const pinned = pinnedLocalClient(LOCAL.id, () => ({ id: 7, name: 'Yurij', email: 'me@example.com' }));
		const bus = createDomainEvents();
		const seen: DomainEvent[] = [];
		bus.on((event) => seen.push(event));
		installDomainEvents(pinned, bus, () => LOCAL.id);
		const api = createDataApi(pinned, 'tmgr.notes', 'tmgr.notes', 'Notes', false, LOCAL.id);

		const created = await api.createRoutine({ title: 'Watered', description: null, date: null, time: null });
		await api.updateRoutine(created.id, { title: 'Watered plants' });

		expect(seen.map((e) => [e.type, (e as any).routineId, e.actor])).toEqual([
			['routine.created', created.id, 'plugin:tmgr.notes'],
			['routine.updated', created.id, 'plugin:tmgr.notes'],
		]);
	});
});
