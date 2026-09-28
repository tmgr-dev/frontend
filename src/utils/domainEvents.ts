import type { AxiosInstance } from 'axios';

type Entity = Record<string, any>;

/** `actor` is set when a plugin caused the write, so the host can keep a plugin from reacting to itself. */
export type DomainEvent = (
	| {
			type: 'task.created' | 'task.updated';
			workspaceId: number | null;
			taskId: number;
			task: Entity;
			/** Only on task.updated: the task fields named in the write's request body. */
			changed?: string[];
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
	| { type: 'comment.deleted'; workspaceId: number | null; commentId: number; taskId?: number }
	| {
			type: 'comment.reactionChanged';
			workspaceId: number | null;
			commentId: number;
			taskId?: number;
			reactions: Entity[];
	  }
	| {
			type: 'task.relationChanged';
			workspaceId: number | null;
			taskId: number;
			otherTaskId: number;
			relationType: string | number;
			change: 'added' | 'removed';
	  }
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

const KNOWN_TASK_FIELDS = new Set([
	'title',
	'description',
	'description_json',
	'status_id',
	'project_category_id',
	'priority',
	'approximately_time',
	'checkpoints',
	'settings',
	'expired_at',
	'common_time',
]);

/** Only the task fields the write's own request body named; a status-shortcut route with no body names none. */
const changedFields = (body: Entity): string[] =>
	Object.keys(body).filter((key) => KNOWN_TASK_FIELDS.has(key));

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
	const withTask = (
		type: 'task.created' | 'task.updated',
		id: number,
		changed?: string[],
	) =>
		payload
			? [
					{
						type,
						workspaceId: workspaceOf(payload),
						taskId: id,
						task: payload,
						...(changed ? { changed } : {}),
					},
			  ]
			: [];

	let match: RegExpMatchArray | null;
	if (method === 'post' && path === 'tasks' && payload?.id) {
		return withTask('task.created', Number(payload.id));
	}
	if ((match = path.match(/^tasks\/(\d+)$/))) {
		const taskId = Number(match[1]);
		if (method === 'delete')
			return [{ type: 'task.deleted', workspaceId: workspaceOf(), taskId }];
		const events: DomainEvent[] = withTask(
			'task.updated',
			taskId,
			changedFields(body),
		);
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
		const events: DomainEvent[] = withTask(
			'task.updated',
			taskId,
			changedFields(body),
		);
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
					...(payload?.task_id != null ? { taskId: Number(payload.task_id) } : {}),
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
	if (
		(match = path.match(/^comments\/(\d+)\/reactions\/toggle$/)) &&
		method === 'post'
	) {
		const raw = payload as unknown;
		const reactions = Array.isArray(raw)
			? raw
			: Array.isArray((raw as Entity)?.reactions)
			? (raw as Entity).reactions
			: [];
		const taskId = Array.isArray(raw) ? undefined : (raw as Entity)?.task_id;
		return [
			{
				type: 'comment.reactionChanged',
				workspaceId: workspaceOf(),
				commentId: Number(match[1]),
				...(taskId != null ? { taskId: Number(taskId) } : {}),
				reactions,
			},
		];
	}
	if (
		(match = path.match(/^tasks\/(\d+)\/related-to\/(\d+)\/with\/(\d+)$/)) &&
		(method === 'post' || method === 'delete')
	) {
		const relationTypeName = (payload as Entity)?.relation_type?.name;
		return [
			{
				type: 'task.relationChanged',
				workspaceId: workspaceOf(),
				taskId: Number(match[1]),
				otherTaskId: Number(match[2]),
				relationType: relationTypeName ?? Number(match[3]),
				change: method === 'post' ? 'added' : 'removed',
			},
		];
	}
	return [];
};

/** Turns every successful write through `instance` (cloud API or the local workspace adapter) into events. */
const SENT_IN = Symbol('workspace the request was sent in');

export const installDomainEvents = (
	instance: AxiosInstance,
	bus: ReturnType<typeof createDomainEvents>,
	currentWorkspaceId: () => number | null,
) => {
	// A slow response can arrive after a workspace switch; its events belong to the workspace it was sent in.
	instance.interceptors.request.use((config) => {
		(config as any)[SENT_IN] = currentWorkspaceId();
		return config;
	});
	return instance.interceptors.response.use((response) => {
		try {
			const plugin = response.config.headers?.['X-TMGR-Plugin'];
			// Never a header: the socket path must not let anything in the request choose its own actor.
			const localAccessActor = (response.config as any).localAccessActor;
			const actor = plugin ? `plugin:${plugin}` : localAccessActor ? `persona:${localAccessActor}` : undefined;
			const sentIn = (response.config as any)[SENT_IN];
			const workspaceId = () =>
				sentIn === undefined ? currentWorkspaceId() : sentIn;
			eventsForResponse(response, workspaceId).forEach((event) =>
				bus.emit(actor ? { ...event, actor } : event),
			);
		} catch (error) {
			console.error(error);
		}
		return response;
	});
};
