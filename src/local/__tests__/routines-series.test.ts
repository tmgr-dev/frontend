import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { materializeDueInstances } from '../routines/service';
import { migrate } from '../schema';
import type { LocalContext } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('local routine series edits on SQLite', () => {
	let ctx: LocalContext;
	const clock = new Date('2026-09-26T10:00:00');
	const api = createLocalApi();
	const call = async (method: string, url: string, body?: unknown) => {
		const res = await dispatchLocal(api, ctx, method, url, body);
		if (!res) throw new Error(`no local route for ${method} ${url}`);
		return res;
	};
	const data = async (method: string, url: string, body?: unknown) =>
		(await call(method, url, body)).data.data;
	const rows = (sql: string, params: any[] = []) =>
		ctx.db.select<any>(sql, params);
	const segments = (id: number) =>
		rows(
			`SELECT valid_from, valid_to, anchor_date, title, frequency, is_recurring, scheduled_time FROM routine_pattern_segments WHERE routine_id = ? ORDER BY valid_from`,
			[id],
		);
	const pattern = async (id: number) =>
		(
			await rows(`SELECT * FROM routine_patterns WHERE routine_id = ?`, [id])
		)[0];
	const title = async (id: number) =>
		(await rows(`SELECT title FROM routines WHERE id = ?`, [id]))[0].title;
	const put = (id: number, body: any) =>
		call('PUT', `daily-routines/tasks/${id}`, body);
	const expand = (from: string, to: string) =>
		data('GET', `daily-routines/expand?from=${from}&to=${to}`);

	const daily = (start: string, extra: any = {}) => ({
		frequency: 'DAILY',
		interval: 1,
		start_date: start,
		time: '09:00',
		...extra,
	});
	const create = async (name: string, recurrence: any) =>
		(
			await data('POST', 'daily-routines/tasks/recurring', {
				title: name,
				recurrence,
			})
		).id as number;

	beforeEach(async () => {
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

	it('following at the first occurrence updates in place without a segment', async () => {
		const id = await create('Run', daily('2026-09-26'));
		const res = await put(id, { title: 'Jog', scope: 'following', op_id: 'a' });
		expect(res.status).toBe(200);
		expect(await title(id)).toBe('Jog');
		expect(await segments(id)).toEqual([]);
		expect(res.data.data.series_version).toBe(1);
	});

	it('following splits, freezes the past and is idempotent on replay', async () => {
		const id = await create('Run', daily('2026-09-20'));
		const body = {
			title: 'Jog',
			scope: 'following',
			occurrence_date: '2026-09-24',
			op_id: 'op-1',
		};
		await put(id, body);
		await put(id, { ...body, title: 'Other' });
		expect(await title(id)).toBe('Jog');
		const segs = await segments(id);
		expect(segs).toHaveLength(1);
		expect(segs[0]).toMatchObject({
			valid_from: '2026-09-20',
			valid_to: '2026-09-23',
			anchor_date: '2026-09-20',
			title: 'Run',
		});
		expect((await pattern(id)).series_version).toBe(1);
	});

	it('expand shows the frozen title in the past and the live one after the split', async () => {
		const id = await create('Run', daily('2026-09-20'));
		await put(id, {
			title: 'Jog',
			scope: 'following',
			occurrence_date: '2026-09-24',
		});
		const entries = await expand('2026-09-22', '2026-09-25');
		expect(entries.map((e: any) => [e.date, e.title])).toEqual([
			['2026-09-22', 'Run'],
			['2026-09-23', 'Run'],
			['2026-09-24', 'Jog'],
			['2026-09-25', 'Jog'],
		]);
	});

	it('an edit from an older segment overlays only the edited field on later versions', async () => {
		const id = await create('Run', daily('2026-09-10'));
		await put(id, {
			title: 'Jog',
			scope: 'following',
			occurrence_date: '2026-09-20',
		});
		await put(id, {
			description: 'Notes',
			scope: 'following',
			occurrence_date: '2026-09-15',
		});
		const entries = await expand('2026-09-14', '2026-09-21');
		const byDate = Object.fromEntries(entries.map((e: any) => [e.date, e]));
		expect(byDate['2026-09-14'].description).toBeNull();
		expect(byDate['2026-09-16'].description).toBe('Notes');
		expect(byDate['2026-09-16'].title).toBe('Run');
		expect(byDate['2026-09-21'].description).toBe('Notes');
		expect(byDate['2026-09-21'].title).toBe('Jog');
	});

	it('a split deletes only scheduler pending rows and keeps overrides and history', async () => {
		const id = await create('Run', daily('2026-09-20'));
		const now = clock.toISOString();
		const ins = (
			d: string,
			status: string,
			ov: number,
			movedFrom: string | null,
		) =>
			ctx.db.execute(
				`INSERT INTO routine_instances (routine_id, scheduled_for, status, is_override, moved_from, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)`,
				[id, `${d}T09:00:00`, status, ov, movedFrom, now, now],
			);
		await ins('2026-09-27', 'PENDING', 0, null);
		await ins('2026-09-28', 'PENDING', 1, null);
		await ins('2026-09-29', 'COMPLETED', 0, null);
		await ins('2026-09-30', 'PENDING', 0, '2026-09-26');
		await put(id, {
			title: 'Jog',
			scope: 'following',
			occurrence_date: '2026-09-27',
		});
		const left = await rows(
			`SELECT substr(scheduled_for,1,10) AS d FROM routine_instances WHERE routine_id = ? ORDER BY d`,
			[id],
		);
		expect(left.map((r) => r.d)).toEqual([
			'2026-09-28',
			'2026-09-29',
			'2026-09-30',
		]);
	});

	it('turning repeat off with following ends the series and keeps the past', async () => {
		const id = await create('Run', daily('2026-09-20'));
		await put(id, {
			is_recurring: false,
			scope: 'following',
			occurrence_date: '2026-09-24',
		});
		expect((await pattern(id)).end_at).toBe('2026-09-23');
		const entries = await expand('2026-09-22', '2026-09-26');
		expect(entries.map((e: any) => e.date)).toEqual([
			'2026-09-22',
			'2026-09-23',
		]);
	});

	it('scope all rewrites the live pattern and deletes segments', async () => {
		const id = await create('Run', daily('2026-09-20'));
		await put(id, {
			title: 'Jog',
			scope: 'following',
			occurrence_date: '2026-09-24',
		});
		await put(id, { title: 'All', scope: 'all' });
		expect(await segments(id)).toEqual([]);
		expect(await title(id)).toBe('All');
		expect((await pattern(id)).series_version).toBe(2);
	});

	it('a missing scope splits at today and keeps the anchor', async () => {
		const id = await create('Run', daily('2026-09-20'));
		await put(id, { title: 'Jog' });
		const segs = await segments(id);
		expect(segs).toHaveLength(1);
		expect(segs[0]).toMatchObject({
			valid_to: '2026-09-25',
			anchor_date: '2026-09-20',
		});
		expect((await pattern(id)).start_at).toBe('2026-09-20');
	});

	it('every two weeks keeps the phase after a title edit on another day', async () => {
		const id = await create('Gym', {
			frequency: 'WEEKLY',
			interval: 2,
			days_of_week: ['MON'],
			start_date: '2026-09-07',
			time: '09:00',
		});
		await put(id, {
			title: 'Gym 2',
			scope: 'following',
			occurrence_date: '2026-09-23',
		});
		const entries = await expand('2026-09-20', '2026-10-12');
		expect(entries.map((e: any) => e.date)).toEqual([
			'2026-09-21',
			'2026-10-05',
		]);
	});

	it('a recurrence change re-anchors at the split date', async () => {
		const id = await create('Run', daily('2026-09-20'));
		await put(id, {
			scope: 'following',
			occurrence_date: '2026-09-24',
			recurrence: { frequency: 'DAILY', interval: 2 },
		});
		expect((await pattern(id)).start_at).toBe('2026-09-24');
		const entries = await expand('2026-09-24', '2026-09-29');
		expect(entries.map((e: any) => e.date)).toEqual([
			'2026-09-24',
			'2026-09-26',
			'2026-09-28',
		]);
	});

	it('non versioned fields apply in place without a segment or a version bump', async () => {
		const id = await create('Run', daily('2026-09-20'));
		await put(id, {
			priority: 'high',
			scope: 'following',
			occurrence_date: '2026-09-24',
		});
		expect(await segments(id)).toEqual([]);
		expect((await pattern(id)).series_version).toBe(0);
		expect(
			(await rows(`SELECT priority FROM routines WHERE id = ?`, [id]))[0]
				.priority,
		).toBe('high');
	});

	it('a stale base_version is 409 for following and for all', async () => {
		const id = await create('Run', daily('2026-09-20'));
		await put(id, {
			title: 'Jog',
			scope: 'following',
			occurrence_date: '2026-09-24',
		});
		const f = await put(id, {
			title: 'X',
			scope: 'following',
			occurrence_date: '2026-09-25',
			base_version: 0,
		});
		const a = await put(id, { title: 'Y', scope: 'all', base_version: 0 });
		expect(f.status).toBe(409);
		expect(a.status).toBe(409);
		expect(await title(id)).toBe('Jog');
	});

	it('single stores title, description and duration on the instance', async () => {
		const id = await create('Run', daily('2026-09-20'));
		await put(id, {
			title: 'Once',
			description: 'Body',
			approximately_time: 45,
			scope: 'single',
			occurrence_date: '2026-09-24',
			op_id: 'op-1',
		});
		const [inst] = await rows(
			`SELECT * FROM routine_instances WHERE routine_id = ?`,
			[id],
		);
		expect(inst).toMatchObject({
			title: 'Once',
			description: 'Body',
			duration_min: 45,
			is_override: 1,
		});
		expect(inst.scheduled_for).toBe('2026-09-24T09:00:00');
		expect(await title(id)).toBe('Run');
		expect(await segments(id)).toEqual([]);
		expect((await pattern(id)).series_version).toBe(1);
		await put(id, {
			title: 'Twice',
			scope: 'single',
			occurrence_date: '2026-09-24',
			op_id: 'op-1',
		});
		expect(
			(
				await rows(`SELECT title FROM routine_instances WHERE routine_id = ?`, [
					id,
				])
			)[0].title,
		).toBe('Once');
		const e = (await expand('2026-09-24', '2026-09-24'))[0];
		expect(e).toMatchObject({
			title: 'Once',
			description: 'Body',
			duration_min: 45,
			time: '09:00',
		});
	});

	it('single with a new time moves the occurrence and hides the original day', async () => {
		const id = await create('Run', daily('2026-09-20'));
		await put(id, {
			recurrence: { time: { hours: 18, minutes: 30 } },
			scope: 'single',
			occurrence_date: '2026-09-24',
			op_id: 'op-1',
		});
		const [inst] = await rows(
			`SELECT * FROM routine_instances WHERE routine_id = ?`,
			[id],
		);
		expect(inst).toMatchObject({
			scheduled_for: '2026-09-24T18:30:00',
			moved_from: '2026-09-24',
		});
		const entries = await expand('2026-09-24', '2026-09-24');
		expect(entries).toHaveLength(1);
		expect(entries[0]).toMatchObject({
			time: '18:30',
			moved_from: '2026-09-24',
			virtual: false,
		});
	});

	it('single moved to another date shows on the new day only', async () => {
		const id = await create('Run', daily('2026-09-20'));
		await put(id, {
			scheduled_date: '2026-09-30',
			scope: 'single',
			occurrence_date: '2026-09-24',
		});
		const entries = await expand('2026-09-23', '2026-09-30');
		const days = entries.map((e: any) => e.date);
		expect(days).not.toContain('2026-09-24');
		expect(days.filter((d: string) => d === '2026-09-30')).toHaveLength(2);
	});

	it('single refuses a recurrence change and a missing occurrence_date', async () => {
		const id = await create('Run', daily('2026-09-20'));
		const a = await put(id, {
			recurrence: { frequency: 'WEEKLY', interval: 1, days_of_week: ['MON'] },
			scope: 'single',
			occurrence_date: '2026-09-24',
		});
		const b = await put(id, { title: 'x', scope: 'single' });
		expect(a.status).toBe(422);
		expect(b.status).toBe(422);
	});

	it('GET with a date returns the governing version and the occurrence override', async () => {
		const id = await create('Old', daily('2026-09-20'));
		await put(id, {
			title: 'New',
			scope: 'following',
			occurrence_date: '2026-09-24',
		});
		await put(id, {
			title: 'Once',
			scope: 'single',
			occurrence_date: '2026-09-25',
		});
		const past = await data(
			'GET',
			`daily-routines/tasks/${id}?date=2026-09-22`,
		);
		expect(past.title).toBe('Old');
		expect(past.series_version).toBe(2);
		expect(past.occurrence).toBeUndefined();
		const day = await data('GET', `daily-routines/tasks/${id}?date=2026-09-25`);
		expect(day.title).toBe('New');
		expect(day.occurrence).toMatchObject({
			date: '2026-09-25',
			title: 'Once',
			status: 'PENDING',
			moved_from: null,
		});
		expect(day.recurrence.frequency).toBe('DAILY');
	});

	it('delete following from an older segment leaves nothing after the day', async () => {
		const id = await create('Run', daily('2026-09-10'));
		await put(id, {
			title: 'Jog',
			scope: 'following',
			occurrence_date: '2026-09-20',
		});
		const res = await call(
			'DELETE',
			`daily-routines/tasks/${id}?scope=following&occurrence_date=2026-09-15`,
		);
		expect(res.status).toBe(204);
		const entries = await expand('2026-09-13', '2026-09-22');
		expect(entries.map((e: any) => e.date)).toEqual([
			'2026-09-13',
			'2026-09-14',
		]);
		expect((await pattern(id)).end_at).toBe('2026-09-14');
	});

	it('delete following at the first occurrence deletes the routine', async () => {
		const id = await create('Run', daily('2026-09-20'));
		await call(
			'DELETE',
			`daily-routines/tasks/${id}?scope=following&occurrence_date=2026-09-20`,
		);
		expect((await call('GET', `daily-routines/tasks/${id}`)).status).toBe(404);
	});

	it('delete following drops pending rows by original day and keeps completed ones', async () => {
		const id = await create('Run', daily('2026-09-20'));
		const now = clock.toISOString();
		await ctx.db.execute(
			`INSERT INTO routine_instances (routine_id, scheduled_for, status, moved_from, created_at, updated_at) VALUES (?, '2026-09-23T09:00:00', 'PENDING', '2026-09-26', ?, ?)`,
			[id, now, now],
		);
		await ctx.db.execute(
			`INSERT INTO routine_instances (routine_id, scheduled_for, status, created_at, updated_at) VALUES (?, '2026-09-27T09:00:00', 'COMPLETED', ?, ?)`,
			[id, now, now],
		);
		await call(
			'DELETE',
			`daily-routines/tasks/${id}?scope=following&occurrence_date=2026-09-25`,
		);
		const left = await rows(
			`SELECT status FROM routine_instances WHERE routine_id = ?`,
			[id],
		);
		expect(left.map((r) => r.status)).toEqual(['COMPLETED']);
	});

	it('delete single skips one occurrence and replays by op_id', async () => {
		const id = await create('Run', daily('2026-09-20'));
		await call(
			'DELETE',
			`daily-routines/tasks/${id}?scope=single&occurrence_date=2026-09-24&op_id=o1`,
		);
		await call(
			'DELETE',
			`daily-routines/tasks/${id}?scope=single&occurrence_date=2026-09-24&op_id=o1`,
		);
		const insts = await rows(
			`SELECT status FROM routine_instances WHERE routine_id = ?`,
			[id],
		);
		expect(insts).toEqual([{ status: 'SKIPPED' }]);
		const e = (await expand('2026-09-24', '2026-09-24'))[0];
		expect(e.status).toBe('skipped');
		expect(
			(await call('DELETE', `daily-routines/tasks/${id}?scope=single`)).status,
		).toBe(422);
	});

	it('delete without scope soft deletes the whole routine', async () => {
		const id = await create('Run', daily('2026-09-20'));
		await call('DELETE', `daily-routines/tasks/${id}`);
		const [row] = await rows(`SELECT deleted_at FROM routines WHERE id = ?`, [
			id,
		]);
		expect(row.deleted_at).not.toBeNull();
	});

	it('shows a gap after a stopped series is split again', async () => {
		const id = await create(
			'Run',
			daily('2026-09-10', { end_date: '2026-09-18' }),
		);
		await put(id, {
			title: 'Jog',
			scope: 'following',
			occurrence_date: '2026-09-15',
		});
		const segs = await segments(id);
		expect(segs.map((s) => [s.valid_from, s.valid_to, s.is_recurring])).toEqual(
			[['2026-09-10', '2026-09-14', 1]],
		);
		const entries = await expand('2026-09-13', '2026-09-22');
		expect(entries.map((e: any) => e.date)).toEqual([
			'2026-09-13',
			'2026-09-14',
			'2026-09-15',
			'2026-09-16',
			'2026-09-17',
			'2026-09-18',
		]);
	});

	it('keeps a completed occurrence visible on a day the new version does not fire', async () => {
		const id = await create('Run', daily('2026-09-20'));
		const now = clock.toISOString();
		await ctx.db.execute(
			`INSERT INTO routine_instances (routine_id, scheduled_for, status, completed_at, created_at, updated_at) VALUES (?, '2026-09-27T09:00:00', 'COMPLETED', ?, ?, ?)`,
			[id, now, now, now],
		);
		await put(id, {
			scope: 'following',
			occurrence_date: '2026-09-26',
			recurrence: { frequency: 'WEEKLY', interval: 1, days_of_week: ['MON'] },
		});
		const days = (await expand('2026-09-26', '2026-09-28')).map(
			(e: any) => e.date,
		);
		expect(days).toEqual(['2026-09-27', '2026-09-28']);
	});

	it('counts routine comments on expanded entries', async () => {
		const id = await create('Run', daily('2026-09-20'));
		await data('POST', `tasks/${id}/comments`, { message: 'hello' });
		const entries = await expand('2026-09-24', '2026-09-25');
		expect(entries.map((e: any) => e.comments_count)).toEqual([1, 1]);
	});

	it('the legacy pattern route goes through the scoped update', async () => {
		const id = await create('Run', daily('2026-09-20'));
		await call('PUT', `daily-routines/tasks/${id}/pattern`, {
			frequency: 'DAILY',
			interval: 3,
		});
		const segs = await segments(id);
		expect(segs).toHaveLength(1);
		expect((await pattern(id)).interval).toBe(3);
		expect((await pattern(id)).start_at).toBe('2026-09-26');
	});

	it('PATCH with from_date moves one occurrence and keeps its original day in moved_from', async () => {
		const id = await create('Run', daily('2026-09-20'));
		const moved = await data(
			'PATCH',
			`daily-routines/tasks/${id}/instances/virtual`,
			{ scheduled_for: '2026-09-25 18:00:00', from_date: '2026-09-24' },
		);
		expect(moved.moved_from).toBe('2026-09-24');
		const entries = await expand('2026-09-24', '2026-09-25');
		expect(entries.map((e: any) => [e.date, e.time])).toEqual([
			['2026-09-25', '09:00'],
			['2026-09-25', '18:00'],
		]);
		const again = await data(
			'PATCH',
			`daily-routines/tasks/${id}/instances/${moved.id}`,
			{ scheduled_for: '2026-09-27 18:00:00', from_date: '2026-09-25' },
		);
		expect(again.moved_from).toBe('2026-09-24');
	});

	it('rescheduling a stored instance of a recurring routine records the original day', async () => {
		const id = await create('Run', daily('2026-09-20'));
		const inst = await data(
			'POST',
			`daily-routines/tasks/${id}/complete-on?date=2026-09-24`,
		);
		const res = await data(
			'PATCH',
			`daily-routines/tasks/${id}/instances/${inst.instance_id}`,
			{ scheduled_for: '2026-09-28 08:00:00' },
		);
		expect(res.moved_from).toBe('2026-09-24');
	});

	it('the scheduler materializes through the governing version', async () => {
		const id = await create('Run', daily('2026-09-20'));
		await put(id, {
			scope: 'following',
			occurrence_date: '2026-09-26',
			recurrence: { time: '07:30' },
		});
		const created = await materializeDueInstances(ctx);
		expect(created).toBe(2);
		const insts = await rows(
			`SELECT scheduled_for FROM routine_instances WHERE routine_id = ? ORDER BY scheduled_for`,
			[id],
		);
		expect(insts.map((r) => r.scheduled_for)).toEqual([
			'2026-09-26T07:30:00',
			'2026-09-27T07:30:00',
		]);
	});
});
