import fs from 'fs';
import path from 'path';
import ts from 'typescript';

// Compile the real Vite module with only its build-time environment replaced.
function loadPusher(localWorkspaceActive = false) {
	const instances: any[] = [];
	class Echo {
		listeners: Record<string, Function> = {};
		channels = new Map<string, any>();
		connector = {
			pusher: {
				connect: jest.fn(),
				connection: {
					bind: (event: string, fn: Function) => {
						this.listeners[event] = fn;
					},
				},
			},
		};
		disconnect = jest.fn();
		connect = jest.fn();
		leave = jest.fn((name: string) => this.channels.delete(name));
		constructor() {
			instances.push(this);
		}
		private(name: string) {
			return this.channel(name);
		}
		channel(name: string) {
			if (!this.channels.has(name)) {
				const callbacks: Record<string, Function> = {};
				const channel: {
					callbacks: Record<string, Function>;
					listen: jest.Mock;
					error: jest.Mock;
					subscribed: jest.Mock;
				} = {
					callbacks,
					listen: jest.fn((event: string, fn: Function) => {
						callbacks[event] = fn;
						return channel;
					}),
					error: jest.fn(),
					subscribed: jest.fn(),
				};
				this.channels.set(name, channel);
			}
			return this.channels.get(name);
		}
	}
	const source = fs
		.readFileSync(path.join(__dirname, '../usePusher.ts'), 'utf8')
		.replace(/\(import\.meta as any\)\.env/g, '({})');
	const js = ts.transpileModule(source, {
		compilerOptions: {
			module: ts.ModuleKind.CommonJS,
			target: ts.ScriptTarget.ES2020,
			esModuleInterop: true,
		},
	}).outputText;
	const result = { exports: {} as any };
	new Function('require', 'module', 'exports', js)(
		(name: string) =>
			name === 'laravel-echo'
				? Echo
				: name === 'pusher-js'
					? {}
					: name === '@/local/runtime'
						? { hasActiveLocalWorkspace: () => localWorkspaceActive }
						: name === '@/utils/tokenSync'
							? require('../../utils/tokenSync')
							: name === '@/utils/pusherChannelAuthorizer'
								? require('../../utils/pusherChannelAuthorizer')
								: require(name),
		result,
		result.exports,
	);
	return { api: result.exports, instances };
}

beforeEach(() => {
	jest.useFakeTimers();
	Object.defineProperty(globalThis, 'localStorage', {
		configurable: true,
		value: { getItem: () => JSON.stringify({ token: 'test-token' }) },
	});
	jest.spyOn(console, 'log').mockImplementation(() => {});
	jest.spyOn(console, 'warn').mockImplementation(() => {});
	jest.spyOn(console, 'error').mockImplementation(() => {});
});
afterEach(() => {
	jest.useRealTimers();
	jest.restoreAllMocks();
});

test('reconnect preserves both subscriber IDs and removing one leaves the other live', () => {
	const { api, instances } = loadPusher();
	api.setRealtimeUser(3);
	const pusher = api.usePusher();
	const first = jest.fn();
	const second = jest.fn();
	const firstId = pusher.subscribeToWorkspace(7, { onTaskUpdated: first });
	pusher.subscribeToWorkspace(7, { onTaskUpdated: second });
	pusher.reconnect();
	pusher.unsubscribeHandlerFromWorkspace(7, firstId);
	const channel = instances[instances.length - 1].channels.get('App.User.3');
	expect(channel).toBeDefined();
	channel.callbacks['.task.updated']({
		task: { id: 1 },
		action: 'updated',
		workspace_id: 7,
	});
	expect(first).not.toHaveBeenCalled();
	expect(second).toHaveBeenCalledTimes(1);
	expect(instances).toHaveLength(1);
	expect(instances[0].connector.pusher.connect).toHaveBeenCalledTimes(1);
	expect(instances[0].connect).not.toHaveBeenCalled();
});

test('repeated errors schedule one reconnect, canceled by success and logout', () => {
	const { api, instances } = loadPusher();
	const pusher = api.usePusher();
	const connection = instances[0];
	connection.listeners.error({});
	connection.listeners.unavailable();
	expect(jest.getTimerCount()).toBe(1);
	connection.listeners.connected();
	expect(jest.getTimerCount()).toBe(0);
	connection.listeners.error({});
	pusher.disconnect();
	jest.runAllTimers();
	expect(connection.connect).not.toHaveBeenCalled();
	expect(pusher.getConnectionInfo().subscriptions).toEqual([]);
});

test('workspace cleanup and logout/login do not revive old subscriptions', () => {
	const { api, instances } = loadPusher();
	api.setRealtimeUser(3);
	const pusher = api.usePusher();
	const old = jest.fn();
	const current = jest.fn();
	const id = pusher.subscribeToWorkspace(7, { onTaskUpdated: old });
	pusher.unsubscribeHandlerFromWorkspace(7, id);
	pusher.subscribeToWorkspace(8, { onTaskUpdated: current });
	expect(pusher.getConnectionInfo().subscriptions).toEqual([
		'App.Workspace.8',
		'App.User.3',
	]);
	pusher.disconnect();
	expect(pusher.getConnectionInfo().subscriptions).toEqual([]);
	api.setRealtimeUser(3);
	pusher.subscribeToWorkspace(9, { onTaskUpdated: current });
	expect(pusher.getConnectionInfo().subscriptions).toEqual([
		'App.Workspace.8',
		'App.User.3',
		'App.Workspace.9',
	]);
	expect(instances).toHaveLength(2);
	instances[1].channels.get('App.User.3').callbacks['.task.updated']({
		task: { id: 1 },
		action: 'updated',
		workspace_id: 7,
	});
	expect(old).not.toHaveBeenCalled();
});

test('legacy Vuex setup stays lazy and its getter shares the composable transport', () => {
	const { api, instances } = loadPusher();
	const loadModule = (filename: string, resolve: (name: string) => any) => {
		const source = fs.readFileSync(path.join(__dirname, filename), 'utf8');
		const js = ts.transpileModule(source, {
			compilerOptions: {
				module: ts.ModuleKind.CommonJS,
				esModuleInterop: true,
			},
		}).outputText;
		const result = { exports: {} as any };
		new Function('require', 'module', 'exports', js)(
			resolve,
			result,
			result.exports,
		);
		return result.exports.default;
	};
	const factory = loadModule('../../store/plugins/pusher.js', () => api);
	const storeModule = loadModule('../../store/modules/pusher.js', (name) =>
		name === '@/store/plugins/pusher'
			? factory
			: name.includes('token-provider')
				? () => ({})
				: {},
	);
	const state = storeModule.state();
	expect(instances).toHaveLength(0);
	const echo = storeModule.getters.getPusher(state);
	api.usePusher().subscribeToWorkspace(7, {});
	expect(instances).toHaveLength(1);
	expect(echo).toBe(instances[0]);
});

test('late connection callbacks from a logged out session cannot revive retries', () => {
	const { api, instances } = loadPusher();
	const pusher = api.usePusher();
	const old = instances[0];
	pusher.disconnect();
	api.usePusher();
	old.listeners.error({});
	old.listeners.connected();
	expect(jest.getTimerCount()).toBe(0);
	expect(pusher.connectionState.value).toBe('connecting');
});

test('a local workspace keeps its handlers without a realtime channel and receives delivered events', () => {
	const { api, instances } = loadPusher();
	const pusher = api.usePusher();
	const onTaskUpdated = jest.fn();
	const other = jest.fn();
	const id = pusher.subscribeToWorkspace(-3, { onTaskUpdated });
	pusher.subscribeToWorkspace(-4, { onTaskUpdated: other });
	expect(id).not.toBe('');
	expect(instances[0].channels.has('App.Workspace.-3')).toBe(false);
	api.deliverToWorkspace(-3, (h: any) =>
		h.onTaskUpdated?.({ id: 1 }, 'updated'),
	);
	expect(onTaskUpdated).toHaveBeenCalledWith({ id: 1 }, 'updated');
	expect(other).not.toHaveBeenCalled();
	pusher.unsubscribeHandlerFromWorkspace(-3, id);
	api.deliverToWorkspace(-3, (h: any) =>
		h.onTaskUpdated?.({ id: 2 }, 'updated'),
	);
	expect(onTaskUpdated).toHaveBeenCalledTimes(1);
	expect(pusher.getConnectionInfo().subscriptions).toEqual([
		'App.Workspace.-4',
	]);
	expect(instances[0].leave).not.toHaveBeenCalled();
});

test('task.assigned and task.unassigned reach onTaskAssignmentChanged with their type', () => {
	const { api, instances } = loadPusher();
	api.setRealtimeUser(3);
	const pusher = api.usePusher();
	const onTaskAssignmentChanged = jest.fn();
	pusher.subscribeToWorkspace(7, { onTaskAssignmentChanged });
	const channel = instances[0].channels.get('App.User.3');
	const payload = {
		task_id: 4,
		workspace_id: 7,
		user_ids: [1],
		personas: [{ uuid: 'p1', name: 'Reviewer', owner_user_id: 1 }],
	};
	channel.callbacks['.task.assigned'](payload);
	channel.callbacks['.task.unassigned'](payload);
	expect(onTaskAssignmentChanged).toHaveBeenNthCalledWith(1, {
		...payload,
		type: 'assigned',
	});
	expect(onTaskAssignmentChanged).toHaveBeenNthCalledWith(2, {
		...payload,
		type: 'unassigned',
	});
});

test('page events on the user channel reach onPageEvent with their type', () => {
	const { api, instances } = loadPusher();
	api.setRealtimeUser(3);
	const pusher = api.usePusher();
	const onPageEvent = jest.fn();
	pusher.subscribeToWorkspace(7, { onPageEvent });
	const channel = instances[0].channels.get('App.User.3');
	const payload = {
		workspace_id: 7,
		page: { id: 3, slug: 'plan', version: 4 },
	};
	for (const type of [
		'page.created',
		'page.updated',
		'page.deleted',
		'page.purged',
		'page.restored',
		'page.moved',
	]) {
		channel.callbacks[`.${type}`](payload);
		expect(onPageEvent).toHaveBeenLastCalledWith(type, payload);
	}
	expect(onPageEvent).toHaveBeenCalledTimes(6);
});

test('countdown events reach user handlers while a local workspace is active', () => {
	const { api, instances } = loadPusher(true);
	const pusher = api.usePusher();
	const started = jest.fn();
	const stopped = jest.fn();
	pusher.subscribeToUser(3, {
		onTaskCountdownStarted: started,
		onTaskCountdownStopped: stopped,
	});
	const channel = instances[0].channels.get('App.User.3');
	channel.callbacks['.task-countdown-started']({ task: { id: 1 } });
	channel.callbacks['.task-countdown-stopped']({ task: { id: 1 } });
	expect(started).toHaveBeenCalledWith({ id: 1 });
	expect(stopped).toHaveBeenCalledWith({ id: 1 });
});

test('manual reconnect after the retry budget is spent starts a fresh budget', () => {
	const { api, instances } = loadPusher();
	const pusher = api.usePusher();
	for (let i = 0; i < 5; i++) {
		instances[0].listeners.error({});
		jest.runOnlyPendingTimers();
	}
	expect(pusher.getConnectionInfo().reconnectAttempts).toBe(5);
	pusher.reconnect();
	expect(pusher.getConnectionInfo().reconnectAttempts).toBe(0);
});

test('without a cloud token no socket is opened but local deliveries still reach subscribers', () => {
	Object.defineProperty(globalThis, 'localStorage', {
		configurable: true,
		value: { getItem: () => null },
	});
	const { api, instances } = loadPusher();
	const pusher = api.usePusher();
	const started = jest.fn();
	pusher.subscribeToUser(-1, { onTaskCountdownStarted: started });
	pusher.subscribeToWorkspace(-5, { onTaskUpdated: jest.fn() });
	pusher.subscribeToWorkspace(7, { onTaskUpdated: jest.fn() });
	pusher.reconnect();
	api.getSharedEcho();
	api.deliverToUser(-1, (h: any) => h.onTaskCountdownStarted?.({ id: 1 }));

	expect(instances).toHaveLength(0);
	expect(started).toHaveBeenCalledWith({ id: 1 });
});

describe('workspace events on the member channel', () => {
	const setup = (userId: number | null = 3) => {
		const loaded = loadPusher();
		if (userId !== null) loaded.api.setRealtimeUser(userId);
		return { ...loaded, pusher: loaded.api.usePusher() };
	};
	const joined = (instances: any[]) => Array.from(instances[0].channels.keys());

	test('events are dispatched to the subscribers of their workspace only', () => {
		const { instances, pusher } = setup();
		const a = jest.fn();
		const b = jest.fn();
		pusher.subscribeToWorkspace(7, { onTaskUpdated: a });
		pusher.subscribeToWorkspace(8, { onTaskUpdated: b });
		const channel = instances[0].channels.get('App.User.3');
		channel.callbacks['.task.updated']({
			task: { id: 1 },
			action: 'updated',
			workspace_id: 7,
		});
		expect(a).toHaveBeenCalledTimes(1);
		expect(b).not.toHaveBeenCalled();
	});

	test('a reordered column reaches the board of its workspace only', () => {
		const { instances, pusher } = setup();
		const a = jest.fn();
		const b = jest.fn();
		pusher.subscribeToWorkspace(7, { onTasksReordered: a });
		pusher.subscribeToWorkspace(8, { onTasksReordered: b });
		instances[0].channels.get('App.User.3').callbacks['.tasks.reordered']({
			status_id: 12,
			source_instance_id: 'client-3',
			workspace_id: 7,
		});
		expect(a).toHaveBeenCalledWith(12, 'client-3');
		expect(b).not.toHaveBeenCalled();
	});

	test('numeric and string workspace ids select the same subscribers', () => {
		const { instances, pusher } = setup();
		const a = jest.fn();
		pusher.subscribeToWorkspace(7, { onCommentAdded: a });
		const channel = instances[0].channels.get('App.User.3');
		channel.callbacks['.comment-added']({
			comment: { id: 1 },
			workspace_id: 7,
		});
		channel.callbacks['.comment-added']({
			comment: { id: 2 },
			workspace_id: '7',
		});
		channel.callbacks['.comment-added']({
			comment: { id: 3 },
			workspace_id: 'x',
		});
		expect(a).toHaveBeenCalledTimes(2);
	});

	test('an event without a workspace id reaches user handlers only', () => {
		const { instances, pusher } = setup();
		const workspace = jest.fn();
		const user = jest.fn();
		pusher.subscribeToWorkspace(7, { onNotificationCreated: workspace });
		pusher.subscribeToUser(3, { onNotificationCreated: user });
		const channel = instances[0].channels.get('App.User.3');
		channel.callbacks['.notification.created']({ notification: { id: 1 } });
		expect(user).toHaveBeenCalledTimes(1);
		expect(workspace).not.toHaveBeenCalled();
	});

	test('an event with a workspace id still reaches user handlers that bind it', () => {
		const { instances, pusher } = setup();
		const workspace = jest.fn();
		const user = jest.fn();
		pusher.subscribeToWorkspace(7, { onActivityCreated: workspace });
		pusher.subscribeToUser(3, { onActivityCreated: user });
		const channel = instances[0].channels.get('App.User.3');
		channel.callbacks['.activity.created']({
			activity: { id: 5 },
			workspace_id: 7,
		});
		expect(user).toHaveBeenCalledWith({ id: 5 });
		expect(workspace).toHaveBeenCalledWith({ id: 5 });
	});

	test('no workspace channel is ever joined', () => {
		const { instances, pusher } = setup();
		pusher.subscribeToWorkspace(7, {});
		pusher.subscribeToWorkspace(8, {});
		pusher.subscribeToUser(3, {});
		expect(joined(instances)).toEqual(['App.User.3']);
		expect(pusher.getConnectionInfo().subscriptions).toEqual([
			'App.Workspace.7',
			'App.User.3',
			'App.Workspace.8',
		]);
	});

	test('subscribed and error reach workspace subscribers once per event', () => {
		const { instances, pusher } = setup();
		const onReconnect = jest.fn();
		const onError = jest.fn();
		const other = jest.fn();
		const userReconnect = jest.fn();
		pusher.subscribeToWorkspace(7, { onReconnect, onError });
		pusher.subscribeToWorkspace(8, { onReconnect: other });
		pusher.subscribeToUser(3, { onReconnect: userReconnect });
		const channel = instances[0].channels.get('App.User.3');
		channel.subscribed.mock.calls[0][0]();
		channel.error.mock.calls[0][0]({ message: 'boom' });
		expect(onReconnect).toHaveBeenCalledTimes(1);
		expect(other).toHaveBeenCalledTimes(1);
		expect(userReconnect).toHaveBeenCalledTimes(1);
		expect(onError).toHaveBeenCalledTimes(1);
	});

	test('a workspace subscriber added to a live channel gets no reconnect callback', () => {
		const { instances, pusher } = setup();
		pusher.subscribeToWorkspace(7, {});
		const channel = instances[0].channels.get('App.User.3');
		channel.subscribed.mock.calls[0][0]();
		const late = jest.fn();
		pusher.subscribeToWorkspace(8, { onReconnect: late });
		expect(late).not.toHaveBeenCalled();
		expect(channel.subscribed).toHaveBeenCalledTimes(1);
	});

	test('the user channel stays joined for workspace subscribers after the last user handler leaves', () => {
		const { instances, pusher } = setup();
		const board = jest.fn();
		pusher.subscribeToWorkspace(7, { onTaskUpdated: board });
		const id = pusher.subscribeToUser(3, {});
		pusher.unsubscribeHandler('App.User.3', id);
		expect(instances[0].leave).not.toHaveBeenCalled();
		instances[0].channels.get('App.User.3').callbacks['.task.updated']({
			task: { id: 1 },
			action: 'updated',
			workspace_id: 7,
		});
		expect(board).toHaveBeenCalledTimes(1);
		pusher.subscribeToUser(3, {});
		pusher.unsubscribeFromUser(3);
		expect(instances[0].leave).not.toHaveBeenCalled();
		instances[0].channels.get('App.User.3').callbacks['.task.updated']({
			task: { id: 2 },
			action: 'updated',
			workspace_id: 7,
		});
		expect(board).toHaveBeenCalledTimes(2);
	});

	test('the user channel stays joined for user handlers after the last workspace subscriber leaves', () => {
		const { instances, pusher } = setup();
		const notified = jest.fn();
		const id = pusher.subscribeToWorkspace(7, {});
		pusher.subscribeToUser(3, { onNotificationCreated: notified });
		pusher.unsubscribeHandlerFromWorkspace(7, id);
		expect(instances[0].leave).not.toHaveBeenCalled();
		instances[0].channels.get('App.User.3').callbacks['.notification.created']({
			notification: { id: 1 },
		});
		expect(notified).toHaveBeenCalledTimes(1);
	});

	test('the user channel is left once neither user handlers nor workspace subscribers remain', () => {
		const { instances, pusher } = setup();
		const wsId = pusher.subscribeToWorkspace(7, {});
		const userId = pusher.subscribeToUser(3, {});
		pusher.unsubscribeHandlerFromWorkspace(7, wsId);
		pusher.unsubscribeHandler('App.User.3', userId);
		expect(instances[0].leave).toHaveBeenCalledWith('App.User.3');
		expect(pusher.getConnectionInfo().subscriptions).toEqual([]);
	});

	test('unsubscribing a whole workspace releases the user channel when nothing else uses it', () => {
		const { instances, pusher } = setup();
		pusher.subscribeToWorkspace(7, {});
		pusher.unsubscribeFromWorkspace(7);
		expect(instances[0].leave).toHaveBeenCalledWith('App.User.3');
		expect(pusher.getConnectionInfo().subscriptions).toEqual([]);
	});

	test('workspace subscribers registered before the user is known attach when it arrives', () => {
		const { api, instances, pusher } = setup(null);
		const board = jest.fn();
		pusher.subscribeToWorkspace(7, { onTaskUpdated: board });
		expect(joined(instances)).toEqual([]);
		api.setRealtimeUser(3);
		const channel = instances[0].channels.get('App.User.3');
		expect(channel).toBeDefined();
		channel.callbacks['.task.updated']({
			task: { id: 1 },
			action: 'updated',
			workspace_id: 7,
		});
		expect(board).toHaveBeenCalledTimes(1);
	});

	test('a user switch moves workspace subscribers to the new user channel', () => {
		const { api, instances, pusher } = setup();
		const board = jest.fn();
		pusher.subscribeToWorkspace(7, { onTaskUpdated: board });
		api.setRealtimeUser(4);
		expect(instances[0].leave).toHaveBeenCalledWith('App.User.3');
		expect(joined(instances)).toEqual(['App.User.4']);
		instances[0].channels.get('App.User.4').callbacks['.task.updated']({
			task: { id: 1 },
			action: 'updated',
			workspace_id: 7,
		});
		expect(board).toHaveBeenCalledTimes(1);
	});

	test('a user switch keeps the old channel while it still has user handlers', () => {
		const { api, instances, pusher } = setup();
		pusher.subscribeToWorkspace(7, {});
		pusher.subscribeToUser(3, {});
		api.setRealtimeUser(4);
		expect(instances[0].leave).not.toHaveBeenCalled();
		expect(joined(instances)).toEqual(['App.User.3', 'App.User.4']);
	});

	test('disconnecting resets the user and every subscriber', () => {
		const { api, instances, pusher } = setup();
		pusher.subscribeToWorkspace(7, {});
		api.disconnectRealtime();
		expect(pusher.getConnectionInfo().subscriptions).toEqual([]);
		pusher.subscribeToWorkspace(8, {});
		expect(instances).toHaveLength(1);
		expect(instances[0].channels.size).toBe(0);
	});

	test('the same user logging back in gets a live feed on the new connection', () => {
		const { api, instances, pusher } = setup();
		pusher.subscribeToWorkspace(7, {});
		api.disconnectRealtime();
		api.setRealtimeUser(3);
		const board = jest.fn();
		pusher.subscribeToWorkspace(7, { onTaskUpdated: board });
		expect(instances).toHaveLength(2);
		const channel = instances[1].channels.get('App.User.3');
		expect(channel).toBeDefined();
		channel.callbacks['.task.updated']({
			task: { id: 1 },
			action: 'updated',
			workspace_id: 7,
		});
		expect(board).toHaveBeenCalledTimes(1);
	});

	describe('handlers of views that stay mounted across a disconnect', () => {
		const fire = (instance: any, userId: number, workspaceId: number) =>
			instance.channels.get(`App.User.${userId}`).callbacks['.task.updated']({
				task: { id: 1 },
				action: 'updated',
				workspace_id: workspaceId,
			});

		test('the same user logging back in gets workspace and user handlers back', () => {
			const { api, instances, pusher } = setup();
			const board = jest.fn();
			const notified = jest.fn();
			pusher.subscribeToWorkspace(7, { onTaskUpdated: board });
			pusher.subscribeToUser(3, { onNotificationCreated: notified });
			api.disconnectRealtime();
			api.setRealtimeUser(null);
			api.setRealtimeUser(3);
			expect(instances).toHaveLength(2);
			fire(instances[1], 3, 7);
			instances[1].channels
				.get('App.User.3')
				.callbacks['.notification.created']({ notification: { id: 1 } });
			expect(board).toHaveBeenCalledTimes(1);
			expect(notified).toHaveBeenCalledTimes(1);
		});

		test('another user logging in inherits nothing from the previous one', () => {
			const { api, instances, pusher } = setup();
			const board = jest.fn();
			const notified = jest.fn();
			pusher.subscribeToWorkspace(7, { onTaskUpdated: board });
			pusher.subscribeToUser(3, { onNotificationCreated: notified });
			api.disconnectRealtime();
			api.setRealtimeUser(null);
			api.setRealtimeUser(4);
			pusher.subscribeToWorkspace(8, {});
			expect(pusher.getConnectionInfo().subscriptions).toEqual([
				'App.Workspace.8',
				'App.User.4',
			]);
			fire(instances[1], 4, 7);
			expect(board).not.toHaveBeenCalled();
			api.setRealtimeUser(null);
			api.setRealtimeUser(3);
			expect(pusher.getConnectionInfo().subscriptions).not.toContain(
				'App.Workspace.7',
			);
		});

		test('a direct switch to another user drops the previous handlers too', () => {
			const { api, instances, pusher } = setup();
			const board = jest.fn();
			pusher.subscribeToWorkspace(7, { onTaskUpdated: board });
			api.disconnectRealtime();
			api.setRealtimeUser(4);
			expect(pusher.getConnectionInfo().subscriptions).toEqual([]);
			expect(instances).toHaveLength(1);
		});

		test('a view unmounted while signed out is not resurrected', () => {
			const { api, instances, pusher } = setup();
			const board = jest.fn();
			const id = pusher.subscribeToWorkspace(7, { onTaskUpdated: board });
			api.disconnectRealtime();
			pusher.unsubscribeHandlerFromWorkspace(7, id);
			api.setRealtimeUser(3);
			expect(pusher.getConnectionInfo().subscriptions).toEqual([]);
			expect(instances[instances.length - 1].channels.size).toBe(0);
		});

		test('a token that belongs to another account never restores them', () => {
			const { api, pusher } = setup();
			const board = jest.fn();
			pusher.subscribeToWorkspace(7, { onTaskUpdated: board });
			api.disconnectRealtime();
			const jwt = (sub: number) =>
				[
					'h',
					Buffer.from(JSON.stringify({ sub })).toString('base64url'),
					's',
				].join('.');
			Object.defineProperty(globalThis, 'localStorage', {
				configurable: true,
				value: { getItem: () => JSON.stringify({ token: jwt(4) }) },
			});
			api.setRealtimeUser(3);
			expect(pusher.getConnectionInfo().subscriptions).toEqual([]);
			expect(board).not.toHaveBeenCalled();
		});

		test('a handler subscribed while signed out survives a repeated disconnect', () => {
			const { api, pusher } = setup();
			pusher.subscribeToWorkspace(7, {});
			api.disconnectRealtime();
			pusher.subscribeToWorkspace(7, {});
			api.disconnectRealtime();
			api.setRealtimeUser(3);
			expect(pusher.getConnectionInfo().subscriptions).toContain(
				'App.Workspace.7',
			);
		});

		test('a second disconnect keeps what the first one remembered', () => {
			const { api, instances, pusher } = setup();
			const board = jest.fn();
			pusher.subscribeToWorkspace(7, { onTaskUpdated: board });
			api.disconnectRealtime();
			api.disconnectRealtime();
			api.setRealtimeUser(3);
			fire(instances[1], 3, 7);
			expect(board).toHaveBeenCalledTimes(1);
		});

		test('a handler keeps its subscription id after coming back', () => {
			const { api, pusher } = setup();
			const id = pusher.subscribeToWorkspace(7, {});
			api.disconnectRealtime();
			api.setRealtimeUser(3);
			pusher.unsubscribeHandlerFromWorkspace(7, id);
			expect(pusher.getConnectionInfo().subscriptions).toEqual([]);
		});
	});

	test('a user handler registered without a token keeps its id when the channel is joined', () => {
		Object.defineProperty(globalThis, 'localStorage', {
			configurable: true,
			value: { getItem: () => null },
		});
		const { api, instances } = loadPusher();
		const pusher = api.usePusher();
		const notified = jest.fn();
		const board = jest.fn();
		const userId = pusher.subscribeToUser(3, {
			onNotificationCreated: notified,
		});
		pusher.subscribeToWorkspace(7, { onTaskUpdated: board });
		Object.defineProperty(globalThis, 'localStorage', {
			configurable: true,
			value: { getItem: () => JSON.stringify({ token: 'test-token' }) },
		});
		api.setRealtimeUser(3);
		const channel = instances[0].channels.get('App.User.3');
		expect(channel).toBeDefined();
		channel.callbacks['.notification.created']({ notification: { id: 1 } });
		channel.callbacks['.task.updated']({
			task: { id: 1 },
			action: 'updated',
			workspace_id: 7,
		});
		expect(notified).toHaveBeenCalledTimes(1);
		expect(board).toHaveBeenCalledTimes(1);
		pusher.unsubscribeHandler('App.User.3', userId);
		expect(instances[0].leave).not.toHaveBeenCalled();
	});

	test('local workspaces never open the user channel', () => {
		const { instances, pusher } = setup();
		pusher.subscribeToWorkspace(-3, {});
		expect(joined(instances)).toEqual([]);
	});
});
