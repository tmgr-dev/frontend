import type { DomainEvent } from '@/utils/domainEvents';

export const DOMAIN_EVENT_CHANNEL = 'tmgr://domain-event';

interface RelayPayload {
	source: string;
	event: DomainEvent;
}

interface Bus {
	on(handler: (event: DomainEvent) => void): () => void;
	emit(event: DomainEvent): void;
}

interface RelayOptions {
	label: string;
	emit: (channel: string, payload: RelayPayload) => unknown;
	listen: (
		channel: string,
		handler: (message: { payload: RelayPayload }) => void,
	) => unknown;
	onRelayed?: (event: DomainEvent) => void;
}

export const installWindowEventRelay = (
	bus: Bus,
	{ label, emit, listen, onRelayed }: RelayOptions,
) => {
	const relayed = new WeakSet<object>();
	const offBus = bus.on((event) => {
		if (relayed.has(event)) return;
		void Promise.resolve(emit(DOMAIN_EVENT_CHANNEL, { source: label, event })).catch(
			(error) => console.error('[window-relay] emit failed', error),
		);
	});
	const listening = Promise.resolve(
		listen(DOMAIN_EVENT_CHANNEL, ({ payload }) => {
			if (!payload?.event || payload.source === label) return;
			const event: DomainEvent = {
				...payload.event,
				actor: payload.event.actor ?? `window:${payload.source}`,
			};
			relayed.add(event);
			onRelayed?.(event);
			bus.emit(event);
		}),
	);
	return () => {
		offBus();
		void listening.then((unlisten) => {
			if (typeof unlisten === 'function') unlisten();
		});
	};
};

interface RelayedEffects {
	invalidate: (key: string | RegExp) => void;
	reloadActiveTasks: () => void;
	deliverTimer?: (
		event: Extract<DomainEvent, { type: 'timer.started' | 'timer.stopped' }>,
	) => void;
}

export const applyRelayedEvent = (
	event: DomainEvent,
	{ invalidate, reloadActiveTasks, deliverTimer }: RelayedEffects,
) => {
	if (event.type.startsWith('comment.')) {
		const taskId = (event as { taskId?: number }).taskId;
		if (taskId != null) invalidate(`comments-task-${taskId}`);
		return;
	}
	if (!event.type.startsWith('task.') && !event.type.startsWith('timer.')) return;
	const { taskId } = event as { taskId: number };
	invalidate(`task-${taskId}`);
	invalidate(/^tasks-status-/);
	if (event.type === 'task.relationChanged') invalidate(`task-${event.otherTaskId}`);
	if (event.type === 'timer.started' || event.type === 'timer.stopped') {
		reloadActiveTasks();
		deliverTimer?.(event);
	}
};
