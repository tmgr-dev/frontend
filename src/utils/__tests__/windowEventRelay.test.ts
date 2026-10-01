import { createDomainEvents, type DomainEvent } from '../domainEvents';
import { installLocalLiveUpdates } from '@/local/liveUpdates';
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

	it('hands timer events to the timer delivery, and only them', () => {
		const deliverTimer = jest.fn();
		const timer: DomainEvent = {
			type: 'timer.stopped',
			workspaceId: -2,
			taskId: 4,
			task: { id: 4 },
		};
		applyRelayedEvent(timer, {
			invalidate: jest.fn(),
			reloadActiveTasks: jest.fn(),
			deliverTimer,
		});
		applyRelayedEvent(event, {
			invalidate: jest.fn(),
			reloadActiveTasks: jest.fn(),
			deliverTimer,
		});
		expect(deliverTimer).toHaveBeenCalledTimes(1);
		expect(deliverTimer).toHaveBeenCalledWith(timer);
	});
});

describe('page events across windows', () => {
	const pageEvent = (actor?: string): DomainEvent => ({
		type: 'page.updated',
		workspaceId: -2,
		pageId: 4,
		page: { id: 4, slug: 'doc', version: 3 },
		...(actor ? { actor } : {}),
	});

	const windows = () => {
		const listeners = new Map<string, (message: { payload: any }) => void>();
		const open = (label: string) => {
			const bus = createDomainEvents();
			installWindowEventRelay(bus, {
				label,
				emit: async (channel, payload) => {
					for (const [other, deliver] of listeners)
						if (other !== label) deliver({ payload });
				},
				listen: async (_channel, handler) => {
					listeners.set(label, handler);
					return () => {};
				},
			});
			const onPageEvent = jest.fn();
			const deps = {
				deliver: jest.fn((workspaceId: number, call: (h: any) => void) => {
					if (workspaceId === -2) call({ onPageEvent });
				}),
				fetchTask: jest.fn(),
				invalidate: jest.fn(),
			};
			installLocalLiveUpdates(deps, bus);
			return { bus, onPageEvent, deps };
		};
		return { main: open('main'), pageWindow: open('page-doc-1') };
	};

	it('refreshes an open page window when the main window writes a page for a socket persona', async () => {
		const { main, pageWindow } = windows();
		await Promise.resolve();
		main.bus.emit(pageEvent('persona:p-1'));
		expect(pageWindow.onPageEvent).toHaveBeenCalledTimes(1);
		expect(pageWindow.onPageEvent).toHaveBeenCalledWith('page.updated', {
			page: { id: 4, slug: 'doc', version: 3 },
		});
		expect(pageWindow.deps.invalidate).toHaveBeenCalledWith(/^pages-/);
	});

	it('refreshes it for a write from the main window UI too, and the main window for a page window write', async () => {
		const { main, pageWindow } = windows();
		await Promise.resolve();
		main.bus.emit(pageEvent());
		expect(pageWindow.onPageEvent).toHaveBeenCalledTimes(1);
		pageWindow.bus.emit(pageEvent());
		expect(main.onPageEvent).toHaveBeenCalledTimes(1);
	});
});
