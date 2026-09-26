import type { AxiosInstance } from 'axios';

type Entity = Record<string, any>;

/** `actor` is set when a plugin caused the write, so the host can keep a plugin from reacting to itself. */
export type DomainEvent = (
	| {
			type: 'task.created' | 'task.updated';
			workspaceId: number | null;
			taskId: number;
			task: Entity;
	  }
	| { type: 'task.deleted'; workspaceId: number | null; taskId: number }
	| {
			type: 'task.statusChanged';
			workspaceId: number | null;
			taskId: number;
			statusId: number;
			task?: Entity;
	  }
	| {
			type: 'timer.started' | 'timer.stopped';
			workspaceId: number | null;
			taskId: number;
			task: Entity;
	  }
	| {
			type: 'comment.created' | 'comment.updated';
			workspaceId: number | null;
			taskId: number;
			comment: Entity;
	  }
	| { type: 'comment.deleted'; workspaceId: number | null; commentId: number }
) & { actor?: string };

export type DomainEventHandler = (event: DomainEvent) => void;

export const createDomainEvents = () => {
	const subscribers = new Set<{
		handler: DomainEventHandler;
		onError: (error: unknown) => void;
	}>();
	return {
		on(
			handler: DomainEventHandler,
			onError: (error: unknown) => void = console.error,
		) {
			const subscriber = { handler, onError };
			subscribers.add(subscriber);
			return () => {
				subscribers.delete(subscriber);
			};
		},
		emit(event: DomainEvent) {
			subscribers.forEach(({ handler, onError }) => {
				try {
					handler(event);
				} catch (error) {
					onError(error);
				}
			});
		},
	};
};

export const domainEvents = createDomainEvents();

interface ObservedResponse {
	config: {
		method?: string;
		url?: string;
		baseURL?: string;
		data?: unknown;
		params?: Record<string, unknown>;
	};
	data: unknown;
}

const pathOf = ({ url = '', baseURL = '' }: ObservedResponse['config']) =>
	(url.startsWith(baseURL) ? url.slice(baseURL.length) : url)
		.split('?')[0]
		.replace(/^https?:\/\/[^/]+/, '')
		.replace(/^\/?(api\/)?/, '')
		.replace(/^\/+|\/+$/g, '');

const bodyOf = (data: unknown): Entity => {
	if (typeof data !== 'string') return (data as Entity) ?? {};
	try {
		return JSON.parse(data) ?? {};
	} catch {
		return {};
	}
};

const TASK_SUBRESOURCES = new Set(['time', 'settings']);

/** The domain events a successful API write stands for. Pure: reads nothing but its arguments. */
export const eventsForResponse = (
	response: ObservedResponse,
	currentWorkspaceId: () => number | null,
): DomainEvent[] => {
	const method = (response.config.method ?? 'get').toLowerCase();
	if (method === 'get') return [];
	const path = pathOf(response.config);
	const body = bodyOf(response.config.data);
	const payload = bodyOf(response.data).data as Entity | undefined;
	const requested = Number(response.config.params?.workspace_id);
	const workspaceOf = (entity?: Entity) =>
		typeof entity?.workspace_id === 'number'
			? entity.workspace_id
			: Number.isFinite(requested) &&
			  response.config.params?.workspace_id != null
			? requested
			: currentWorkspaceId();
	const withTask = (type: 'task.created' | 'task.updated', id: number) =>
		payload
			? [{ type, workspaceId: workspaceOf(payload), taskId: id, task: payload }]
			: [];

	let match: RegExpMatchArray | null;
	if (method === 'post' && path === 'tasks' && payload?.id) {
		return withTask('task.created', Number(payload.id));
	}
	if ((match = path.match(/^tasks\/(\d+)$/))) {
		const taskId = Number(match[1]);
		if (method === 'delete')
			return [{ type: 'task.deleted', workspaceId: workspaceOf(), taskId }];
		const events: DomainEvent[] = withTask('task.updated', taskId);
		if ('status_id' in body && payload?.status_id != null) {
			events.push({
				type: 'task.statusChanged',
				workspaceId: workspaceOf(payload),
				taskId,
				statusId: Number(payload.status_id),
				task: payload,
			});
		}
		return events;
	}
	if ((match = path.match(/^tasks\/(\d+)\/countdown$/)) && payload) {
		return [
			{
				type: method === 'delete' ? 'timer.stopped' : 'timer.started',
				workspaceId: workspaceOf(payload),
				taskId: Number(match[1]),
				task: payload,
			},
		];
	}
	if (
		(match = path.match(/^tasks\/(\d+)\/comments$/)) &&
		method === 'post' &&
		payload
	) {
		return [
			{
				type: 'comment.created',
				workspaceId: workspaceOf(payload),
				taskId: Number(match[1]),
				comment: payload,
			},
		];
	}
	if (
		(match = path.match(/^tasks\/(\d+)\/([a-z_-]+)$/)) &&
		method === 'put' &&
		payload
	) {
		const taskId = Number(match[1]);
		const events: DomainEvent[] = withTask('task.updated', taskId);
		if (!TASK_SUBRESOURCES.has(match[2]) && payload.status_id != null) {
			events.push({
				type: 'task.statusChanged',
				workspaceId: workspaceOf(payload),
				taskId,
				statusId: Number(payload.status_id),
				task: payload,
			});
		}
		return events;
	}
	if ((match = path.match(/^statuses\/(\d+)\/tasks$/)) && method === 'put') {
		const statusId = Number(match[1]);
		return (Array.isArray(body.task_ids) ? body.task_ids : []).map(
			(id: unknown) => ({
				type: 'task.statusChanged' as const,
				workspaceId: workspaceOf(),
				taskId: Number(id),
				statusId,
			}),
		);
	}
	if ((match = path.match(/^comments\/(\d+)$/))) {
		if (method === 'delete') {
			return [
				{
					type: 'comment.deleted',
					workspaceId: workspaceOf(),
					commentId: Number(match[1]),
				},
			];
		}
		if (method === 'put' && payload) {
			return [
				{
					type: 'comment.updated',
					workspaceId: workspaceOf(payload),
					taskId: Number(payload.task_id),
					comment: payload,
				},
			];
		}
	}
	return [];
};

/** Turns every successful write through `instance` (cloud API or the local workspace adapter) into events. */
export const installDomainEvents = (
	instance: AxiosInstance,
	bus: ReturnType<typeof createDomainEvents>,
	currentWorkspaceId: () => number | null,
) =>
	instance.interceptors.response.use((response) => {
		try {
			const plugin = response.config.headers?.['X-TMGR-Plugin'];
			eventsForResponse(response, currentWorkspaceId).forEach((event) =>
				bus.emit(plugin ? { ...event, actor: `plugin:${plugin}` } : event),
			);
		} catch (error) {
			console.error(error);
		}
		return response;
	});
