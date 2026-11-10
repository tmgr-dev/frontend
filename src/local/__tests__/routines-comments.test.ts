import { createLocalApi } from '../api';
import { dispatchLocal } from '../dispatch';
import { migrate } from '../schema';
import type { LocalContext } from '../types';
import { memoryDb, nodeSqliteAvailable } from './nodeDb';

const describeSqlite = nodeSqliteAvailable ? describe : describe.skip;

describeSqlite('local routine comments on SQLite', () => {
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

	it('creates, lists, edits and deletes routine comments', async () => {
		const routine = await data('POST', 'daily-routines/tasks', {
			title: 'Run',
		});
		const created = await call('POST', `tasks/${routine.id}/comments`, {
			message: ' hello ',
		});
		expect(created.status).toBe(201);
		const comment = created.data.data;
		expect(comment).toMatchObject({
			task_id: routine.id,
			message: 'hello',
			reactions: [],
		});
		expect(comment.id).toBeGreaterThan(1_000_000_000);
		expect(
			(await data('GET', `tasks/${routine.id}/comments`)).map((c: any) => c.id),
		).toEqual([comment.id]);
		const edited = await data('PUT', `comments/${comment.id}`, {
			message: 'bye',
		});
		expect(edited.message).toBe('bye');
		expect(await data('DELETE', `comments/${comment.id}`)).toMatchObject({
			success: true,
			task_id: routine.id,
		});
		expect(await data('GET', `tasks/${routine.id}/comments`)).toEqual([]);
		expect(
			(await data('GET', `daily-routines/tasks/${routine.id}`)).comments_count,
		).toBe(0);
	});

	it('refuses reactions, blank messages and unknown routines', async () => {
		const routine = await data('POST', 'daily-routines/tasks', {
			title: 'Run',
		});
		const comment = await data('POST', `tasks/${routine.id}/comments`, {
			message: 'x',
		});
		const reaction = await call(
			'POST',
			`comments/${comment.id}/reactions/toggle`,
			{ emoji: '+1' },
		);
		expect(reaction.status).toBe(422);
		expect(
			(await call('POST', `tasks/${routine.id}/comments`, { message: ' ' }))
				.status,
		).toBe(422);
		expect(
			(
				await call('POST', `tasks/${routine.id + 50}/comments`, {
					message: 'x',
				})
			).status,
		).toBe(404);
		expect(
			(await call('PUT', `comments/${comment.id + 50}`, { message: 'x' }))
				.status,
		).toBe(404);
	});

	it('keeps routine comment ids apart from task comment ids', async () => {
		const routine = await data('POST', 'daily-routines/tasks', {
			title: 'Run',
		});
		const task = await data('POST', 'tasks', { title: 'Task' });
		const taskComment = await data('POST', `tasks/${task.id}/comments`, {
			message: 'on task',
		});
		const routineComment = await data('POST', `tasks/${routine.id}/comments`, {
			message: 'on routine',
		});
		const second = await data('POST', `tasks/${routine.id}/comments`, {
			message: 'again',
		});
		expect(taskComment.id).toBeLessThan(1_000_000_000);
		expect(routineComment.id).toBe(1_000_000_001);
		expect(second.id).toBe(1_000_000_002);
		expect(
			(await data('GET', `tasks/${task.id}/comments`)).map(
				(c: any) => c.message,
			),
		).toEqual(['on task']);
		expect(
			(await data('PUT', `comments/${taskComment.id}`, { message: 'edited' }))
				.task_id,
		).toBe(task.id);
		const expanded = await data(
			'GET',
			'daily-routines/expand?from=2026-09-26&to=2026-09-26',
		);
		expect(expanded[0].comments_count).toBe(2);
	});
});
