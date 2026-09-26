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
		).toEqual([{ type: 'task.updated', workspaceId: -42, taskId: 7, task }]);
		expect(
			eventsForResponse(
				response('put', 'tasks/7/time', task, { common_time: 60 }),
				current,
			),
		).toEqual([{ type: 'task.updated', workspaceId: -42, taskId: 7, task }]);
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
			{ type: 'task.updated', workspaceId: -42, taskId: 7, task },
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
			{ type: 'task.updated', workspaceId: -42, taskId: 7, task },
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
				response('delete', '/comments/3', { success: true }),
				current,
			),
		).toEqual([{ type: 'comment.deleted', workspaceId: 5, commentId: 3 }]);
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
