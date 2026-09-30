import { createDomainEvents, type DomainEvent } from '../domainEvents';
import { applyRelayedEvent, installWindowEventRelay } from '../windowEventRelay';

const event: DomainEvent = { type: 'task.deleted', workspaceId: -2, taskId: 4 };

const setup = (label = 'main') => {
	const bus = createDomainEvents();
	const emit = jest.fn(async () => undefined);
	let incoming: (message: { payload: any }) => void = () => {};
	const listen = jest.fn(async (_channel: string, handler: any) => {
		incoming = handler;
		return () => {};
	});
	const onRelayed = jest.fn();
	installWindowEventRelay(bus, { label, emit, listen, onRelayed });
	const seen: DomainEvent[] = [];
	bus.on((e) => seen.push(e));
	return { bus, emit, onRelayed, seen, receive: (payload: any) => incoming({ payload }) };
};

describe('installWindowEventRelay', () => {
	it('sends local events to other windows with the source label', () => {
		const { bus, emit } = setup('task-a-1');
		bus.emit(event);
		expect(emit).toHaveBeenCalledWith('tmgr://domain-event', { source: 'task-a-1', event });
	});

	it('delivers events from other windows with a window actor', () => {
		const { receive, seen, onRelayed } = setup();
		receive({ source: 'task-a-1', event });
		expect(seen).toEqual([{ ...event, actor: 'window:task-a-1' }]);
		expect(onRelayed).toHaveBeenCalledWith({ ...event, actor: 'window:task-a-1' });
	});

	it('keeps an actor the event already had', () => {
		const { receive, seen } = setup();
		receive({ source: 'task-a-1', event: { ...event, actor: 'plugin:x' } });
		expect(seen[0].actor).toBe('plugin:x');
	});

	it('never echoes a relayed event back out', () => {
		const { receive, emit } = setup();
		receive({ source: 'task-a-1', event });
		expect(emit).not.toHaveBeenCalled();
	});

	it('ignores events from its own window', () => {
		const { receive, seen, onRelayed } = setup('main');
		receive({ source: 'main', event });
		expect(seen).toEqual([]);
		expect(onRelayed).not.toHaveBeenCalled();
	});
});

describe('applyRelayedEvent', () => {
	const run = (event: DomainEvent) => {
		const invalidate = jest.fn();
		const reloadActiveTasks = jest.fn();
		applyRelayedEvent(event, { invalidate, reloadActiveTasks });
		return { invalidate, reloadActiveTasks };
	};

	it('drops the task and status list caches for task events', () => {
		const { invalidate, reloadActiveTasks } = run(event);
		expect(invalidate).toHaveBeenCalledWith('task-4');
		expect(invalidate).toHaveBeenCalledWith(/^tasks-status-/);
		expect(reloadActiveTasks).not.toHaveBeenCalled();
	});

	it('reloads active tasks when a timer starts or stops', () => {
		const { reloadActiveTasks } = run({
			type: 'timer.started',
			workspaceId: 1,
			taskId: 4,
			task: {},
		});
		expect(reloadActiveTasks).toHaveBeenCalledTimes(1);
	});

	it('drops the comment cache for comment events', () => {
		const { invalidate } = run({ type: 'comment.created', workspaceId: 1, taskId: 4, comment: {} });
		expect(invalidate).toHaveBeenCalledWith('comments-task-4');
	});

	it('ignores routine events', () => {
		const { invalidate } = run({ type: 'routine.deleted', workspaceId: 1, routineId: 2 });
		expect(invalidate).not.toHaveBeenCalled();
	});
});
