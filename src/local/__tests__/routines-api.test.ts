import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { migrate } from '../schema';
import type { LocalContext } from '../types';
import { materializeDueInstances, ROUTINE_ID_BASE } from '../routines/service';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('local daily-routines API on SQLite', () => {
	let ctx: LocalContext;
	let clock = new Date('2026-09-26T10:00:00');
	const api = createLocalApi();
	const call = async (method: string, url: string, body?: unknown) => {
		const res = await dispatchLocal(api, ctx, method, url, body);
		if (!res) throw new Error(`no local route for ${method} ${url}`);
		return res;
	};
	const data = async (method: string, url: string, body?: unknown) => (await call(method, url, body)).data.data;

	beforeEach(async () => {
		clock = new Date('2026-09-26T10:00:00');
		ctx = {
			db: memoryDb(),
			workspace: {
				id: -42,
				name: 'Personal',
				code: 'local-personal',
				schema_version: 0,
				created_at: '',
				path: '/tmp/x',
				database: '/tmp/x/workspace.db',
			},
			user: { id: 7, name: 'Yurij', email: 'me@example.com' },
			now: () => clock,
			files: {
				url: (key: string) => `tmgrfile://localhost/test/${key}`,
				read: async () => new Blob(['bytes']),
				remove: async () => {},
			},
		};
		await migrate(ctx.db, clock.toISOString());
	});

	it('resolves the workspace id and starts every count at zero', async () => {
		expect(await data('GET', 'daily-routines/workspace')).toEqual({ workspace_id: -42 });
		expect(await data('GET', 'daily-routines/tasks/count')).toEqual({ count: 0 });
		expect(await data('GET', 'daily-routines/tasks/archived/count')).toEqual({ count: 0 });
		expect(await data('GET', 'daily-routines/tasks/completed/count')).toEqual({ count: 0 });
		expect(await data('GET', 'daily-routines/tasks')).toEqual([]);
	});

	it('creates a plain routine with a disjoint id and rejects a blank title', async () => {
		const routine = await data('POST', 'daily-routines/tasks', { title: 'Water plants', description: 'x' });
		expect(routine.id).toBeGreaterThan(ROUTINE_ID_BASE);
		expect(routine.title).toBe('Water plants');
		expect(routine.is_daily_routine).toBe(true);
		expect(routine.daily_routine).toBe(true);
		expect(routine.workspace_id).toBe(-42);
		expect((await call('POST', 'daily-routines/tasks', { title: '  ' })).status).toBe(422);

		const second = await data('POST', 'daily-routines/tasks', { title: 'Second' });
		expect(second.id).toBe(routine.id + 1);
	});

	it('creates a recurring routine with a pattern and an initial instance', async () => {
		// The initial instance's time comes from the request's own scheduled_time, not
		// from recurrence.time — matching the Java DTO (RoutineTaskRequest.resolveScheduledTime()).
		const created = await call('POST', 'daily-routines/tasks/recurring', {
			title: 'Standup',
			recurrence: { frequency: 'DAILY', interval: 1, time: { hours: 9, minutes: 30 } },
			scheduled_date: '2026-09-26',
			scheduled_time: { hours: 9, minutes: 30 },
		});
		expect(created.status).toBe(201);
		const routine = created.data.data;
		const instances = await data('GET', `daily-routines/tasks/${routine.id}/instances`);
		expect(instances).toHaveLength(1);
		expect(instances[0].scheduled_for).toBe('2026-09-26T09:30:00Z');
	});

	it('quick-creates a routine scheduled today when no date is given', async () => {
		const created = await data('POST', 'daily-routines/tasks/quick', { title: 'Quick one' });
		const instances = await data('GET', `daily-routines/tasks/${created.id}/instances`);
		expect(instances[0].scheduled_date).toBe('2026-09-26');
		expect(instances[0].scheduled_time).toBe('00:00:00');

		const withTime = await data('POST', 'daily-routines/tasks/quick', {
			title: 'Timed',
			date: '2026-10-01',
			time: '08:15',
		});
		const timedInstances = await data('GET', `daily-routines/tasks/${withTime.id}/instances`);
		expect(timedInstances[0].scheduled_for).toBe('2026-10-01T08:15:00Z');
	});

	it('lists only active (non-archived, non-deleted) routines newest first', async () => {
		const a = await data('POST', 'daily-routines/tasks', { title: 'A' });
		const b = await data('POST', 'daily-routines/tasks', { title: 'B' });
		await call('POST', `daily-routines/tasks/${a.id}/archive`);
		expect((await data('GET', 'daily-routines/tasks')).map((r: any) => r.id)).toEqual([b.id]);
		expect(await data('GET', 'daily-routines/tasks/count')).toEqual({ count: 1 });
		expect(await data('GET', 'daily-routines/tasks/archived/count')).toEqual({ count: 1 });
	});

	it('404s reading, updating, completing or listing instances of an archived or missing routine', async () => {
		const routine = await data('POST', 'daily-routines/tasks', { title: 'Gone soon' });
		await call('POST', `daily-routines/tasks/${routine.id}/archive`);
		expect((await call('GET', `daily-routines/tasks/${routine.id}`)).status).toBe(404);
		expect((await call('PUT', `daily-routines/tasks/${routine.id}`, { title: 'x' })).status).toBe(404);
		expect((await call('POST', `daily-routines/tasks/${routine.id}/complete`)).status).toBe(404);
		expect((await call('GET', `daily-routines/tasks/${routine.id}/instances`)).status).toBe(404);
		expect((await call('GET', 'daily-routines/tasks/999999999999')).status).toBe(404);
	});

	it('updates title/description/category/duration and reflects them back', async () => {
		const routine = await data('POST', 'daily-routines/tasks', { title: 'Draft' });
		const updated = await data('PUT', `daily-routines/tasks/${routine.id}`, {
			title: 'Final',
			description: 'Notes',
			routine_category: 'health',
			approximately_time: 45,
		});
		expect(updated).toMatchObject({
			title: 'Final',
			description: 'Notes',
			routine_category: 'health',
			approximately_time: 45,
		});
	});

	it('is_recurring:false removes the pattern; a scheduled_date creates or updates the one-off instance', async () => {
		const routine = await call('POST', 'daily-routines/tasks/recurring', {
			title: 'Weekly sync',
			recurrence: { frequency: 'WEEKLY', days_of_week: ['MON'], interval: 1, time: { hours: 9, minutes: 0 } },
		}).then((r) => r.data.data);

		await data('PUT', `daily-routines/tasks/${routine.id}`, {
			title: 'Weekly sync',
			is_recurring: false,
			scheduled_date: '2026-09-28',
			scheduled_time: { hours: 14, minutes: 0 },
		});
		const expanded = await data('GET', 'daily-routines/expand?from=2026-09-28&to=2026-09-28');
		expect(expanded).toHaveLength(1);
		expect(expanded[0].frequency).toBeNull();
		expect(expanded[0].time).toBe('14:00');

		await data('PUT', `daily-routines/tasks/${routine.id}`, {
			title: 'Weekly sync',
			scheduled_date: '2026-09-28',
			scheduled_time: null,
		});
		const unscheduled = await data('GET', 'daily-routines/expand?from=2026-09-28&to=2026-09-28');
		expect(unscheduled[0].time).toBeNull();
	});

	it('deletes a routine (204), after which it is gone from GET and the list', async () => {
		const routine = await data('POST', 'daily-routines/tasks', { title: 'Bye' });
		expect((await call('DELETE', `daily-routines/tasks/${routine.id}`)).status).toBe(204);
		expect((await call('GET', `daily-routines/tasks/${routine.id}`)).status).toBe(404);
		expect(await data('GET', 'daily-routines/tasks')).toEqual([]);
	});

	it('deletes an already-archived routine but not one already deleted', async () => {
		const routine = await data('POST', 'daily-routines/tasks', { title: 'Archived then gone' });
		await call('POST', `daily-routines/tasks/${routine.id}/archive`);
		expect((await call('DELETE', `daily-routines/tasks/${routine.id}`)).status).toBe(204);
		expect((await call('DELETE', `daily-routines/tasks/${routine.id}`)).status).toBe(404);
	});

	it('toggles completion for today via complete, and for an explicit date via complete-on', async () => {
		const routine = await data('POST', 'daily-routines/tasks', { title: 'Meditate' });
		const first = await data('POST', `daily-routines/tasks/${routine.id}/complete`);
		expect(first.status).toBe('COMPLETED');
		const second = await data('POST', `daily-routines/tasks/${routine.id}/complete`);
		expect(second.status).toBe('PENDING');

		const onDate = await data('POST', `daily-routines/tasks/${routine.id}/complete-on`, { date: '2026-10-05' });
		expect(onDate).toMatchObject({ task_id: routine.id, date: '2026-10-05', status: 'COMPLETED', completed: true });
	});

	it('lists, completes, skips and deletes instances by id, scoped to their routine', async () => {
		const routine = await data('POST', 'daily-routines/tasks/recurring', {
			title: 'Meds',
			recurrence: { frequency: 'DAILY', interval: 1 },
			scheduled_date: '2026-09-26',
		});
		const other = await data('POST', 'daily-routines/tasks', { title: 'Other' });
		const [instance] = await data('GET', `daily-routines/tasks/${routine.id}/instances`);

		expect((await call('POST', `daily-routines/tasks/${other.id}/instances/${instance.id}/complete`)).status).toBe(
			404,
		);
		const completed = await data('POST', `daily-routines/tasks/${routine.id}/instances/${instance.id}/complete`);
		expect(completed.status).toBe('COMPLETED');
		const skipped = await data('POST', `daily-routines/tasks/${routine.id}/instances/${instance.id}/skip`);
		expect(skipped.status).toBe('SKIPPED');

		expect((await call('DELETE', `daily-routines/tasks/${routine.id}/instances/${instance.id}`)).status).toBe(204);
		expect(await data('GET', `daily-routines/tasks/${routine.id}/instances`)).toEqual([]);
	});

	it('reschedules a numeric instance (keeping its time when none is given) and a virtual one', async () => {
		const routine = await data('POST', 'daily-routines/tasks/recurring', {
			title: 'Run',
			recurrence: { frequency: 'DAILY', interval: 1, time: { hours: 7, minutes: 0 } },
			scheduled_date: '2026-09-26',
			scheduled_time: { hours: 7, minutes: 0 },
		});
		const [instance] = await data('GET', `daily-routines/tasks/${routine.id}/instances`);

		const moved = await data('PATCH', `daily-routines/tasks/${routine.id}/instances/${instance.id}`, {
			scheduled_for: '2026-09-27',
		});
		expect(moved.scheduled_for).toBe('2026-09-27T07:00:00Z');

		const virtual = await data('PATCH', `daily-routines/tasks/${routine.id}/instances/virtual`, {
			scheduled_for: '2026-09-30 08:15:00',
		});
		expect(virtual.scheduled_for).toBe('2026-09-30T08:15:00Z');
		expect(await data('GET', `daily-routines/tasks/${routine.id}/instances`)).toHaveLength(2);
	});

	it('upserts a recurrence pattern via the legacy PUT .../pattern endpoint', async () => {
		const routine = await data('POST', 'daily-routines/tasks', { title: 'Legacy pattern' });
		const pattern = await data('PUT', `daily-routines/tasks/${routine.id}/pattern`, {
			frequency: 'WEEKLY',
			interval: 2,
			days_of_week: ['mon', 'wed'],
			time: { hours: 6, minutes: 0 },
		});
		expect(pattern).toMatchObject({ task_id: routine.id, frequency: 'WEEKLY', interval: 2 });
		expect(pattern.days_of_week).toEqual(['MON', 'WED']);
		expect((await call('PUT', `daily-routines/tasks/${routine.id}/pattern`, { frequency: 'NONE' })).status).toBe(
			422,
		);
	});

	it('reports task stats and the upcoming instance feed', async () => {
		const routine = await data('POST', 'daily-routines/tasks/recurring', {
			title: 'Journaling',
			recurrence: { frequency: 'DAILY', interval: 1 },
			scheduled_date: '2026-09-26',
		});
		const [instance] = await data('GET', `daily-routines/tasks/${routine.id}/instances`);
		await call('POST', `daily-routines/tasks/${routine.id}/instances/${instance.id}/complete`);

		expect(await data('GET', `daily-routines/tasks/${routine.id}/stats`)).toEqual({
			total: 1,
			completed: 1,
			skipped: 0,
		});
		const upcoming = await data('GET', 'daily-routines/tasks/upcoming?limit=10');
		expect(upcoming.map((i: any) => i.task_id)).toContain(routine.id);
	});

	it('expands a daily pattern across the range and honours the instance status override', async () => {
		const routine = await data('POST', 'daily-routines/tasks/recurring', {
			title: 'Daily walk',
			recurrence: { frequency: 'DAILY', interval: 1, time: { hours: 7, minutes: 0 }, duration_min: 20 },
			scheduled_date: '2026-09-26',
		});
		const [instance] = await data('GET', `daily-routines/tasks/${routine.id}/instances`);
		await call('POST', `daily-routines/tasks/${routine.id}/instances/${instance.id}/complete`);

		const entries = await data('GET', 'daily-routines/expand?from=2026-09-26&to=2026-09-28');
		expect(entries.map((e: any) => e.date)).toEqual(['2026-09-26', '2026-09-27', '2026-09-28']);
		expect(entries[0]).toMatchObject({ status: 'completed', completed: true, virtual: false, instance_id: instance.id });
		expect(entries[1]).toMatchObject({ status: 'pending', completed: false, virtual: true, instance_id: null });
		expect(entries[0].routine_category).toEqual({ id: 'none', name: 'General', color: '#888888' });
	});

	it('expands an orphan instance (pattern removed) and a plain unscheduled routine on today', async () => {
		const orphan = await data('POST', 'daily-routines/tasks/recurring', {
			title: 'Orphan',
			recurrence: { frequency: 'DAILY', interval: 1 },
			scheduled_date: '2026-09-27',
		});
		await data('PUT', `daily-routines/tasks/${orphan.id}`, { title: 'Orphan', is_recurring: false });

		const plain = await data('POST', 'daily-routines/tasks', { title: 'Someday' });

		const entries = await data('GET', 'daily-routines/expand?from=2026-09-26&to=2026-09-27');
		const orphanEntry = entries.find((e: any) => e.task_id === orphan.id);
		expect(orphanEntry).toMatchObject({ date: '2026-09-27', virtual: false, frequency: null });

		const todayEntries = await data('GET', 'daily-routines/expand?from=2026-09-26&to=2026-09-26');
		const plainEntry = todayEntries.find((e: any) => e.task_id === plain.id);
		expect(plainEntry).toMatchObject({ date: '2026-09-26', virtual: true, time: null });
	});

	it('uses device-local "today" for a plain routine, not toISOString().slice(0,10)', async () => {
		// Pick a local instant guaranteed to sit on the other side of the UTC day
		// boundary from the local one, whichever way this machine's TZ leans, so the
		// test would fail if `today` were ever computed via toISOString().slice(0,10).
		const offsetMinutes = new Date(2026, 8, 26).getTimezoneOffset();
		clock =
			offsetMinutes > 0
				? new Date(2026, 8, 26, 23, 45, 0) // local behind UTC: late local = next-day UTC
				: offsetMinutes < 0
					? new Date(2026, 8, 27, 0, 15, 0) // local ahead of UTC: early local = prev-day UTC
					: new Date(2026, 8, 26, 12, 0, 0); // no offset: nothing to prove either way
		const localToday = `${clock.getFullYear()}-${String(clock.getMonth() + 1).padStart(2, '0')}-${String(
			clock.getDate(),
		).padStart(2, '0')}`;
		const utcToday = clock.toISOString().slice(0, 10);
		if (offsetMinutes !== 0) expect(localToday).not.toBe(utcToday);

		const plain = await data('POST', 'daily-routines/tasks', { title: 'Near midnight' });
		const entries = await data('GET', `daily-routines/expand?from=${localToday}&to=${localToday}`);
		expect(entries.find((e: any) => e.task_id === plain.id)?.date).toBe(localToday);
		expect(entries.some((e: any) => e.task_id === plain.id && e.date === utcToday)).toBe(offsetMinutes === 0);
	});

	it('builds year-stats by date from the same expansion', async () => {
		const routine = await data('POST', 'daily-routines/tasks/recurring', {
			title: 'Stretch',
			recurrence: { frequency: 'DAILY', interval: 1 },
			scheduled_date: '2026-09-26',
		});
		const [instance] = await data('GET', `daily-routines/tasks/${routine.id}/instances`);
		await call('POST', `daily-routines/tasks/${routine.id}/instances/${instance.id}/complete`);

		const stats = await data('GET', 'daily-routines/expand/stats?from=2026-09-26&to=2026-09-27');
		expect(stats['2026-09-26']).toEqual({ fires: 1, completed: 1 });
		expect(stats['2026-09-27']).toEqual({ fires: 1, completed: 0 });
	});

	it('converts a routine into a regular task in the current workspace, honouring category and ticket numbering', async () => {
		const category = await data('POST', 'project_categories', { title: 'Habits', code: 'hb' });
		const routine = await data('POST', 'daily-routines/tasks', { title: 'Read', approximately_time: 15 });

		expect(
			(await call('POST', `daily-routines/tasks/${routine.id}/convert`, { workspace_id: 999 })).status,
		).toBe(409);

		const task = await data('POST', `daily-routines/tasks/${routine.id}/convert`, {
			workspace_id: -42,
			project_category_id: category.id,
		});
		expect(task).toMatchObject({ title: 'Read', category_tasks_sequence_id: 1, approximately_time: 15 });
		expect((await call('GET', `daily-routines/tasks/${routine.id}`)).status).toBe(404);
	});

	it('routes PUT/PATCH tasks/:id for a routine id to the routine, honouring only title and approximately_time', async () => {
		const routine = await data('POST', 'daily-routines/tasks', { title: 'Resize me' });
		await call('PUT', `tasks/${routine.id}`, { approximately_time: 90, status_id: 999999 });
		const reloaded = await data('GET', `daily-routines/tasks/${routine.id}`);
		expect(reloaded.approximately_time).toBe(90);
		expect(reloaded.status_id).toBeNull();

		await call('PATCH', `tasks/${routine.id}`, { title: 'Renamed' });
		expect((await data('GET', `daily-routines/tasks/${routine.id}`)).title).toBe('Renamed');
	});

	it('404s GET/DELETE tasks/:id for a routine id instead of touching the tasks table', async () => {
		const routine = await data('POST', 'daily-routines/tasks', { title: 'Untouchable' });
		expect((await call('GET', `tasks/${routine.id}`)).status).toBe(404);
		expect((await call('DELETE', `tasks/${routine.id}`)).status).toBe(404);
		expect((await call('GET', `daily-routines/tasks/${routine.id}`)).status).toBe(200);
	});

	it('materializes the next due instance for a recurring routine', async () => {
		const routine = await data('POST', 'daily-routines/tasks/recurring', {
			title: 'Daily pill',
			recurrence: { frequency: 'DAILY', interval: 1, time: { hours: 8, minutes: 0 } },
		});
		clock = new Date('2026-09-27T07:00:00');
		const created = await materializeDueInstances(ctx);
		expect(created).toBe(1);
		const instances = await data('GET', `daily-routines/tasks/${routine.id}/instances`);
		expect(instances.some((i: any) => i.scheduled_date === '2026-09-27')).toBe(true);

		const again = await materializeDueInstances(ctx);
		expect(again).toBe(0);
	});
});
