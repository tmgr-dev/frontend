import axios from 'axios';
import {
	createDomainEvents,
	eventsForResponse,
	installDomainEvents,
	type DomainEvent,
} from '../domainEvents';

const task = { id: 7, title: 'Ship it', status_id: 3, workspace_id: -42 };
const response = (
	method: string,
	url: string,
	data: unknown,
	body?: unknown,
	params?: Record<string, unknown>,
) => ({
	config: {
		method,
		url,
		baseURL: 'https://api.tmgr.dev/api/',
		data: body === undefined ? undefined : JSON.stringify(body),
		params,
	},
	data: { data },
});

describe('eventsForResponse', () => {
	const current = () => 5;

	it('reports created, updated and deleted tasks with their workspace', () => {
		expect(eventsForResponse(response('post', 'tasks', task), current)).toEqual(
			[{ type: 'task.created', workspaceId: -42, taskId: 7, task }],
		);
		expect(
			eventsForResponse(
				response('put', 'tasks/7', task, { title: 'x' }),
				current,
			),
		).toEqual([
			{ type: 'task.updated', workspaceId: -42, taskId: 7, task, changed: ['title'] },
		]);
		expect(
			eventsForResponse(
				response('put', 'tasks/7/time', task, { common_time: 60 }),
				current,
			),
		).toEqual([
			{ type: 'task.updated', workspaceId: -42, taskId: 7, task, changed: ['common_time'] },
		]);
		expect(
			eventsForResponse(
				response('delete', '/tasks/7', { success: true }),
				current,
			),
		).toEqual([{ type: 'task.deleted', workspaceId: 5, taskId: 7 }]);
	});

	it('adds a status change when the status moves', () => {
		expect(
			eventsForResponse(
				response('patch', 'tasks/7', task, { status_id: 3 }),
				current,
			),
		).toEqual([
			{ type: 'task.updated', workspaceId: -42, taskId: 7, task, changed: ['status_id'] },
			{
				type: 'task.statusChanged',
				workspaceId: -42,
				taskId: 7,
				statusId: 3,
				task,
			},
		]);
		expect(
			eventsForResponse(response('put', 'tasks/7/done', task), current),
		).toEqual([
			{ type: 'task.updated', workspaceId: -42, taskId: 7, task, changed: [] },
			{
				type: 'task.statusChanged',
				workspaceId: -42,
				taskId: 7,
				statusId: 3,
				task,
			},
		]);
		expect(
			eventsForResponse(
				response('put', 'statuses/9/tasks', null, { task_ids: [1, 2] }),
				current,
			),
		).toEqual([
			{ type: 'task.statusChanged', workspaceId: 5, taskId: 1, statusId: 9 },
			{ type: 'task.statusChanged', workspaceId: 5, taskId: 2, statusId: 9 },
		]);
	});

	it('reports timers, using the workspace the request targeted', () => {
		expect(
			eventsForResponse(
				response(
					'post',
					'tasks/7/countdown',
					{ ...task, workspace_id: undefined },
					undefined,
					{
						workspace_id: 12,
					},
				),
				current,
			),
		).toEqual([
			{
				type: 'timer.started',
				workspaceId: 12,
				taskId: 7,
				task: { ...task, workspace_id: undefined },
			},
		]);
		expect(
			eventsForResponse(response('delete', 'tasks/7/countdown', task), current),
		).toEqual([{ type: 'timer.stopped', workspaceId: -42, taskId: 7, task }]);
	});

	it('reports comments', () => {
		const comment = { id: 3, task_id: 7, message: 'hi' };
		expect(
			eventsForResponse(
				response('post', '/tasks/7/comments', comment),
				current,
			),
		).toEqual([
			{ type: 'comment.created', workspaceId: 5, taskId: 7, comment },
		]);
		expect(
			eventsForResponse(response('put', '/comments/3', comment), current),
		).toEqual([
			{ type: 'comment.updated', workspaceId: 5, taskId: 7, comment },
		]);
		expect(
			eventsForResponse(
				response('delete', '/comments/3', { success: true, task_id: 7 }),
				current,
			),
		).toEqual([{ type: 'comment.deleted', workspaceId: 5, commentId: 3, taskId: 7 }]);
		expect(
			eventsForResponse(
				response('delete', '/comments/3', { success: true }),
				current,
			),
		).toEqual([{ type: 'comment.deleted', workspaceId: 5, commentId: 3 }]);
	});

	it('reports a reaction toggle, with the task id when the response carries it', () => {
		const reactions = [{ emoji: '👍', count: 1, reacted: true }];
		expect(
			eventsForResponse(
				response('post', 'comments/3/reactions/toggle', reactions),
				current,
			),
		).toEqual([
			{ type: 'comment.reactionChanged', workspaceId: 5, commentId: 3, reactions },
		]);
		expect(
			eventsForResponse(
				response('post', 'comments/3/reactions/toggle', { reactions, task_id: 7 }),
				current,
			),
		).toEqual([
			{
				type: 'comment.reactionChanged',
				workspaceId: 5,
				commentId: 3,
				taskId: 7,
				reactions,
			},
		]);
	});

	it('reports a relation change, naming the type when the response carries it', () => {
		expect(
			eventsForResponse(
				response('post', 'tasks/7/related-to/9/with/1', {
					id: 1,
					task_id: 7,
					related_task_id: 9,
					task_relation_type_id: 1,
					relation_type: { id: 1, name: 'blocks' },
				}),
				current,
			),
		).toEqual([
			{
				type: 'task.relationChanged',
				workspaceId: 5,
				taskId: 7,
				otherTaskId: 9,
				relationType: 'blocks',
				change: 'added',
			},
		]);
		expect(
			eventsForResponse(
				response('delete', 'tasks/7/related-to/9/with/1', { success: true }),
				current,
			),
		).toEqual([
			{
				type: 'task.relationChanged',
				workspaceId: 5,
				taskId: 7,
				otherTaskId: 9,
				relationType: 1,
				change: 'removed',
			},
		]);
	});

	it('reports routine creation, updates and instance changes, never keyed as task', () => {
		const routine = {
			id: 1_000_000_005,
			title: 'Water plants',
			description: null,
			scheduled_date: '2026-09-26',
			scheduled_time: null,
			workspace_id: -42,
			created_at: 'a',
			updated_at: 'b',
		};
		expect(
			eventsForResponse(response('post', 'daily-routines/tasks', routine), current),
		).toEqual([{ type: 'routine.created', workspaceId: -42, routineId: routine.id, routine }]);
		expect(
			eventsForResponse(
				response('put', `daily-routines/tasks/${routine.id}`, routine),
				current,
			),
		).toEqual([{ type: 'routine.updated', workspaceId: -42, routineId: routine.id, routine }]);
		expect(
			eventsForResponse(
				response('delete', `daily-routines/tasks/${routine.id}`, null),
				current,
			),
		).toEqual([{ type: 'routine.deleted', workspaceId: 5, routineId: routine.id }]);

		const pattern = { id: 9, task_id: routine.id, frequency: 'WEEKLY', interval: 1 };
		expect(
			eventsForResponse(
				response('put', `daily-routines/tasks/${routine.id}/pattern`, pattern),
				current,
			),
		).toEqual([{ type: 'routine.updated', workspaceId: 5, routineId: routine.id }]);

		const instance = {
			id: 4,
			task_id: routine.id,
			scheduled_for: '2026-09-26T09:00:00Z',
			status: 'COMPLETED',
		};
		expect(
			eventsForResponse(
				response('post', `daily-routines/tasks/${routine.id}/instances/4/complete`, instance),
				current,
			),
		).toEqual([{ type: 'routine.updated', workspaceId: 5, routineId: routine.id, instance }]);
		expect(
			eventsForResponse(
				response('patch', `daily-routines/tasks/${routine.id}/instances/virtual`, instance),
				current,
			),
		).toEqual([{ type: 'routine.updated', workspaceId: 5, routineId: routine.id, instance }]);
		expect(
			eventsForResponse(
				response('delete', `daily-routines/tasks/${routine.id}/instances/4`, null),
				current,
			),
		).toEqual([{ type: 'routine.updated', workspaceId: 5, routineId: routine.id }]);

		// complete-on's response is not instance-shaped, so no `instance` rides along.
		expect(
			eventsForResponse(
				response('post', `daily-routines/tasks/${routine.id}/complete-on`, {
					instance_id: 4,
					task_id: routine.id,
					date: '2026-09-26',
					status: 'COMPLETED',
					completed: true,
				}),
				current,
			),
		).toEqual([{ type: 'routine.updated', workspaceId: 5, routineId: routine.id }]);
	});

	it('reports a routine conversion as both routine.deleted and task.created', () => {
		const convertedTask = { id: 77, title: 'Water plants', workspace_id: -42 };
		expect(
			eventsForResponse(
				response('post', 'daily-routines/tasks/1000000005/convert', convertedTask),
				current,
			),
		).toEqual([
			{ type: 'routine.deleted', workspaceId: -42, routineId: 1_000_000_005, taskId: 77 },
			{ type: 'task.created', workspaceId: -42, taskId: 77, task: convertedTask },
		]);
	});

	it('reports PUT/PATCH tasks/:id for a routine-range id as routine.updated, never task.updated', () => {
		const routine = {
			id: 1_000_000_005,
			title: 'Resize me',
			scheduled_date: null,
			workspace_id: -42,
			created_at: 'a',
			updated_at: 'b',
		};
		expect(
			eventsForResponse(response('put', 'tasks/1000000005', routine, { approximately_time: 90 }), current),
		).toEqual([{ type: 'routine.updated', workspaceId: -42, routineId: routine.id, routine }]);
		expect(
			eventsForResponse(response('patch', 'tasks/1000000005', routine, { title: 'x' }), current),
		).toEqual([{ type: 'routine.updated', workspaceId: -42, routineId: routine.id, routine }]);
	});

	it('keeps a cloud task id above the routine range a task', () => {
		const cloudTask = { id: 1_000_000_005, title: 'Big id', workspace_id: 5 };
		expect(
			eventsForResponse(response('patch', 'tasks/1000000005', cloudTask, { title: 'x' }), current).map((e) => e.type),
		).toEqual(['task.updated']);
	});

	it('ignores reads and unrelated writes', () => {
		expect(
			eventsForResponse(response('get', 'tasks/7', task), current),
		).toEqual([]);
		expect(
			eventsForResponse(response('post', 'tasks/7/comments/help', {}), current),
		).toEqual([]);
		expect(
			eventsForResponse(response('put', 'tasks/update-orders', []), current),
		).toEqual([]);
		expect(
			eventsForResponse(response('post', 'ai/optimize', {}), current),
		).toEqual([]);
	});
});

describe('createDomainEvents', () => {
	it('delivers events to subscribers until they unsubscribe', () => {
		const bus = createDomainEvents();
		const seen: DomainEvent[] = [];
		const off = bus.on((event) => seen.push(event));
		const event: DomainEvent = {
			type: 'task.deleted',
			workspaceId: 1,
			taskId: 2,
		};
		bus.emit(event);
		off();
		bus.emit(event);
		expect(seen).toEqual([event]);
	});

	it('keeps delivering when one subscriber throws', () => {
		const bus = createDomainEvents();
		const seen: string[] = [];
		const errors: unknown[] = [];
		const onError = (error: unknown) => errors.push(error);
		bus.on(() => {
			throw new Error('plugin bug');
		}, onError);
		bus.on((event) => seen.push(event.type));
		bus.emit({ type: 'task.deleted', workspaceId: 1, taskId: 2 });
		expect(seen).toEqual(['task.deleted']);
		expect(errors).toHaveLength(1);
	});
});

describe('installDomainEvents', () => {
	it('emits events for successful writes that go through the axios instance', async () => {
		const instance = axios.create({
			baseURL: 'https://api.tmgr.dev/api/',
			adapter: async (config) => ({
				data: { data: task },
				status: 201,
				statusText: 'Created',
				headers: {},
				config,
			}),
		});
		const bus = createDomainEvents();
		const seen: DomainEvent[] = [];
		bus.on((event) => seen.push(event));
		installDomainEvents(instance, bus, () => 5);

		await instance.post('tasks', { title: 'Ship it' });
		await instance.get('tasks/7');

		expect(seen).toEqual([
			{ type: 'task.created', workspaceId: -42, taskId: 7, task },
		]);
	});
});

it('marks events caused by a plugin with that plugin as the actor', async () => {
	const instance = axios.create({
		adapter: async (config) => ({
			data: { data: task },
			status: 200,
			statusText: 'OK',
			headers: {},
			config,
		}),
	});
	const bus = createDomainEvents();
	const seen: DomainEvent[] = [];
	bus.on((event) => seen.push(event));
	installDomainEvents(instance, bus, () => 5);

	await instance.put(
		'tasks/7',
		{ title: 'x' },
		{ headers: { 'X-TMGR-Plugin': 'tmgr.estimate' } },
	);

	expect(seen).toEqual([
		{
			type: 'task.updated',
			workspaceId: -42,
			taskId: 7,
			task,
			changed: ['title'],
			actor: 'plugin:tmgr.estimate',
		},
	]);
});

it('attributes an event to the workspace that was current when the request was sent', async () => {
	let finish: () => void = () => undefined;
	let arrived: () => void = () => undefined;
	const inAdapter = new Promise<void>((resolve) => (arrived = resolve));
	const instance = axios.create({
		adapter: (config) =>
			new Promise((resolve) => {
				arrived();
				finish = () =>
					resolve({
						data: { data: { id: 3, task_id: 7, message: 'hi' } },
						status: 201,
						statusText: 'Created',
						headers: {},
						config,
					});
			}),
	});
	const bus = createDomainEvents();
	const seen: DomainEvent[] = [];
	bus.on((event) => seen.push(event));
	let current = 56;
	installDomainEvents(instance, bus, () => current);

	const request = instance.post('tasks/7/comments', { message: 'hi' });
	await inAdapter;
	current = -42;
	finish();
	await request;

	expect(seen.map((e) => e.workspaceId)).toEqual([56]);
});

describe('page events', () => {
	const page = { id: 4, workspace_id: -42, title: 'Saha', body: 'x', version: 2 };

	it('reports local page writes with the page, and a section write with its section', () => {
		const current = () => -42;
		expect(eventsForResponse(response('post', 'pages', page), current)).toEqual([
			{ type: 'page.created', workspaceId: -42, pageId: 4, page },
		]);
		expect(eventsForResponse(response('patch', 'pages/4', page, { version: 1 }), current)).toEqual([
			{ type: 'page.updated', workspaceId: -42, pageId: 4, page },
		]);
		expect(eventsForResponse(response('post', 'pages/4/append', page, { markdown: 'a' }), current)[0].type).toBe(
			'page.updated',
		);
		expect(eventsForResponse(response('put', 'pages/4/sections/notes', page, { markdown: 'a' }), current)).toEqual([
			{ type: 'page.updated', workspaceId: -42, pageId: 4, page, changedSections: ['notes'] },
		]);
		expect(eventsForResponse(response('post', 'pages/4/restore', page), current)[0].type).toBe('page.restored');
		expect(eventsForResponse(response('post', 'pages/4/move', page, { parent_id: 1 }), current)[0].type).toBe(
			'page.moved',
		);
		expect(eventsForResponse(response('post', 'pages/4/versions/1/restore', page), current)[0].type).toBe(
			'page.updated',
		);
		expect(eventsForResponse(response('delete', 'pages/4', { deleted: 4 }), current)).toEqual([
			{ type: 'page.deleted', workspaceId: -42, pageId: 4 },
		]);
	});

	it('leaves shared workspaces to realtime, so a cloud write is never reported twice', () => {
		const cloud = { ...page, workspace_id: 5 };
		expect(eventsForResponse(response('patch', 'pages/4', cloud, { version: 1 }), () => 5)).toEqual([]);
		expect(eventsForResponse(response('delete', 'pages/4', { deleted: 4 }), () => 5)).toEqual([]);
	});
});
