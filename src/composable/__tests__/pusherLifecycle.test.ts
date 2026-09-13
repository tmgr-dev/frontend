import fs from 'fs';
import path from 'path';
import ts from 'typescript';

// Compile the real Vite module with only its build-time environment replaced.
function loadPusher() {
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
	const pusher = api.usePusher();
	const first = jest.fn();
	const second = jest.fn();
	const firstId = pusher.subscribeToWorkspace(7, { onTaskUpdated: first });
	pusher.subscribeToWorkspace(7, { onTaskUpdated: second });
	pusher.reconnect();
	pusher.unsubscribeHandlerFromWorkspace(7, firstId);
	const channel =
		instances[instances.length - 1].channels.get('App.Workspace.7');
	expect(channel).toBeDefined();
	channel.callbacks['.task.updated']({ task: { id: 1 }, action: 'updated' });
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
	const pusher = api.usePusher();
	const old = jest.fn();
	const current = jest.fn();
	const id = pusher.subscribeToWorkspace(7, { onTaskUpdated: old });
	pusher.unsubscribeHandlerFromWorkspace(7, id);
	pusher.subscribeToWorkspace(8, { onTaskUpdated: current });
	expect(pusher.getConnectionInfo().subscriptions).toEqual(['App.Workspace.8']);
	pusher.disconnect();
	pusher.subscribeToWorkspace(9, { onTaskUpdated: current });
	expect(pusher.getConnectionInfo().subscriptions).toEqual(['App.Workspace.9']);
	expect(instances).toHaveLength(2);
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
