import {
	isFail,
	shouldInterruptAfterDeadline,
	type QuickJSContext,
	type QuickJSHandle,
	type QuickJSWASMModule,
} from 'quickjs-emscripten-core';

export type SandboxErrorCode = 'TIMEOUT' | 'PLUGIN_ERROR' | 'DISPOSED';

export class SandboxError extends Error {
	constructor(readonly code: SandboxErrorCode, message: string) {
		super(message);
	}
}

export type DispatchKind = 'event' | 'command' | 'badges' | 'page' | 'section';

export interface SandboxOptions {
	quickjs: QuickJSWASMModule;
	code: string;
	/** Every guest call to the app, already bound to this plugin's broker. */
	call: (method: string, params: unknown) => Promise<unknown>;
	/** CPU budget for each synchronous slice of plugin code. */
	cpuMs?: number;
	/** Wall-clock budget for a whole dispatch, including awaited host calls. */
	wallMs?: number;
	memoryBytes?: number;
	/** An error in plugin code that runs outside any call from the host (a timer-less background job). */
	onBackgroundError?: (error: SandboxError) => void;
}

/**
 * The only globals a plugin gets. Everything goes through `__host(method, json)`, which the host
 * answers with the broker; `__host` and `__dispatch` are removed before the plugin's code runs.
 */
const MAX_CALL_BYTES = 1024 * 1024;
const MAX_RESULT_BYTES = 2 * 1024 * 1024;
const MAX_MESSAGE = 1000;

const PRELUDE = `(() => {
	const host = globalThis.__host;
	const handlers = { event: new Map(), command: new Map(), badges: new Map(), page: new Map(), section: new Map() };
	const call = (method, params) => {
		const json = JSON.stringify(params === undefined ? null : params);
		if (json.length > ${MAX_CALL_BYTES}) return Promise.reject(new RangeError('call arguments are larger than 1 MB'));
		return host(method, json).then((result) => JSON.parse(result));
	};
	const register = (kind, id, fn) => {
		if (typeof fn !== 'function') throw new TypeError(kind + ' handler must be a function');
		if (kind === 'event') {
			const list = handlers.event.get(id) || [];
			list.push(fn);
			handlers.event.set(id, list);
			if (list.length > 1) return Promise.resolve(null);
		} else {
			handlers[kind].set(id, fn);
		}
		return call('register', { kind, id });
	};
	const text = (value) => (typeof value === 'string' ? value : JSON.stringify(value));
	const logger = (level) => (...args) => { call('log', { level, message: args.map(text).join(' ') }); };
	globalThis.console = Object.freeze({ log: logger('info'), info: logger('info'), warn: logger('warn'), error: logger('error') });
	const freeze = (o) => Object.freeze(o);
	globalThis.tmgr = freeze({
		workspace: freeze({ current: () => call('workspace.current') }),
		settings: freeze({ get: () => call('settings.get') }),
		tasks: freeze({
			list: (query) => call('tasks.list', query || {}),
			get: (id) => call('tasks.get', { id }),
			create: (fields) => call('tasks.create', fields),
			update: (id, patch) => call('tasks.update', { id, patch }),
		}),
		statuses: freeze({ list: () => call('statuses.list') }),
		categories: freeze({ list: () => call('categories.list') }),
		time: freeze({ start: (taskId) => call('time.start', { taskId }), stop: (taskId) => call('time.stop', { taskId }) }),
		comments: freeze({
			list: (taskId) => call('comments.list', { taskId }),
			add: (taskId, text) => call('comments.add', { taskId, text }),
		}),
		storage: freeze({
			get: (key) => call('storage.get', { key }),
			set: (key, value) => call('storage.set', { key, value }),
			delete: (key) => call('storage.delete', { key }),
			keys: () => call('storage.keys'),
		}),
		files: freeze({
			export: (path, content) => call('files.export', { path, content }),
			reveal: (path) => call('files.reveal', { path }),
			list: (taskId) => call('files.list', { taskId }),
			read: (fileId) => call('files.read', { fileId }),
			pick: () => call('files.pick'),
		}),
		net: freeze({
			fetch: (url, init) => {
				const options = init || {};
				return call('net.fetch', { url, method: options.method, headers: options.headers, body: options.body }).then(
					(response) =>
						freeze({
							status: response.status,
							ok: response.status >= 200 && response.status < 300,
							headers: freeze(Object.fromEntries(response.headers.map(([k, v]) => [k.toLowerCase(), v]))),
							text: () => Promise.resolve(response.body),
							json: () => Promise.resolve().then(() => JSON.parse(response.body)),
						}),
				);
			},
		}),
		events: freeze({ on: (type, fn) => register('event', type, fn) }),
		commands: freeze({ register: (id, fn) => register('command', id, fn) }),
		ui: freeze({
			provideBadges: (id, fn) => register('badges', id, fn),
			providePage: (id, fn) => register('page', id, fn),
			provideTaskSection: (id, fn) => register('section', id, fn),
			setStatusBarItem: (id, item) =>
				call('ui.setStatusBarItem', item === null ? { id, text: null } : Object.assign({}, item, { id })),
			notify: (message) => call('ui.notify', { message }),
			refresh: (kind, id) => call('ui.refresh', { kind, id }),
		}),
	});
	globalThis.__dispatch = (kind, id, argsJson) => {
		const args = JSON.parse(argsJson);
		if (kind === 'event') {
			const list = handlers.event.get(id) || [];
			return Promise.all(list.map((fn) => Promise.resolve().then(() => fn(args)))).then(() => 'null');
		}
		const fn = handlers[kind] && handlers[kind].get(id);
		if (!fn) return Promise.reject(new Error('no ' + kind + ' handler for ' + id));
		return Promise.resolve()
			.then(() => fn(args))
			.then((result) => {
				const json = JSON.stringify(result === undefined ? null : result);
				if (json.length > ${MAX_RESULT_BYTES}) throw new RangeError('result is larger than 2 MB');
				return json;
			});
	};
})();
delete globalThis.__host;`;

const isInterrupt = (error: any) =>
	error?.name === 'InternalError' && /interrupted/i.test(error?.message ?? '');

/** One plugin's QuickJS runtime: its own heap, memory and stack limits, a CPU deadline per slice. */
export const createSandbox = ({
	quickjs,
	code,
	call,
	cpuMs = 500,
	wallMs = 15_000,
	memoryBytes = 32 * 1024 * 1024,
	onBackgroundError = () => undefined,
}: SandboxOptions) => {
	const runtime = quickjs.newRuntime();
	runtime.setMemoryLimit(memoryBytes);
	runtime.setMaxStackSize(512 * 1024);
	const ctx: QuickJSContext = runtime.newContext();
	let disposed = false;
	let dispatchFn: QuickJSHandle | null = null;

	const guarded = <T>(slice: () => T): T => {
		runtime.setInterruptHandler(
			shouldInterruptAfterDeadline(Date.now() + cpuMs),
		);
		try {
			return slice();
		} finally {
			runtime.removeInterruptHandler();
		}
	};

	const failure = (handle: QuickJSHandle) => {
		const error = ctx.dump(handle);
		handle.dispose();
		return isInterrupt(error)
			? new SandboxError('TIMEOUT', `plugin code ran longer than ${cpuMs} ms`)
			: new SandboxError(
					'PLUGIN_ERROR',
					(typeof error === 'object' && error
						? `${error.name ?? 'Error'}: ${error.message}`
						: String(error)
					).slice(0, MAX_MESSAGE),
			  );
	};

	const inFlight = new Set<(error: SandboxError) => void>();
	let started = false;
	let startError: SandboxError | null = null;
	const pump = () => {
		if (disposed) return;
		const result = guarded(() => runtime.executePendingJobs());
		if (!result.error) return result.dispose();
		const error = failure(result.error);
		if (inFlight.size === 0) {
			if (started) onBackgroundError(error);
			else startError = error;
		}
		inFlight.forEach((reject) => reject(error));
	};

	const host = ctx.newFunction('__host', (methodHandle, paramsHandle) => {
		const method = ctx.getString(methodHandle);
		const json = ctx.getString(paramsHandle);
		const deferred = ctx.newPromise();
		const request =
			json.length > MAX_CALL_BYTES
				? Promise.reject(
						Object.assign(new Error('call arguments are larger than 1 MB'), {
							code: 'INVALID_PARAMS',
						}),
				  )
				: call(method, JSON.parse(json));
		request.then(
			(result) => {
				if (disposed) return;
				ctx
					.newString(JSON.stringify(result ?? null))
					.consume((value) => deferred.resolve(value));
			},
			(error) => {
				if (disposed) return;
				ctx
					.newError({
						name: typeof error?.code === 'string' ? error.code : 'Error',
						message: (error instanceof Error
							? error.message
							: String(error)
						).slice(0, MAX_MESSAGE),
					})
					.consume((value) => deferred.reject(value));
			},
		);
		deferred.settled.then(() => {
			if (deferred.alive) deferred.dispose();
			pump();
		});
		return deferred.handle;
	});
	ctx.setProp(ctx.global, '__host', host);
	host.dispose();
	const prelude = ctx.evalCode(PRELUDE, 'prelude.js');
	if (prelude.error) throw failure(prelude.error);
	prelude.dispose();
	dispatchFn = ctx.getProp(ctx.global, '__dispatch');
	ctx.unwrapResult(ctx.evalCode('delete globalThis.__dispatch;')).dispose();

	const settle = async (promise: QuickJSHandle): Promise<unknown> => {
		let reject: (error: SandboxError) => void = () => undefined;
		let timer: ReturnType<typeof setTimeout> | undefined;
		const aborted = new Promise<never>((_, fail) => {
			reject = fail;
			timer = setTimeout(
				() =>
					fail(
						new SandboxError(
							'TIMEOUT',
							`plugin did not answer within ${wallMs} ms`,
						),
					),
				wallMs,
			);
		});
		inFlight.add(reject);
		const native = ctx.resolvePromise(promise);
		promise.dispose();
		let result: Awaited<typeof native>;
		try {
			pump();
			result = await Promise.race([native, aborted]);
		} catch (error) {
			native
				.then((late) => (isFail(late) ? late.error : late.value).dispose())
				.catch(() => undefined);
			throw error;
		} finally {
			inFlight.delete(reject);
			clearTimeout(timer);
		}
		if (isFail(result)) throw failure(result.error);
		const json = ctx.getString(result.value);
		result.value.dispose();
		if (json.length > MAX_RESULT_BYTES)
			throw new SandboxError('PLUGIN_ERROR', 'result is larger than 2 MB');
		return JSON.parse(json);
	};

	return {
		async start() {
			const result = guarded(() => ctx.evalCode(code, 'main.js'));
			if (result.error) throw failure(result.error);
			result.dispose();
			pump();
			started = true;
			if (startError) throw startError;
		},
		async dispatch(
			kind: DispatchKind,
			id: string,
			args: unknown,
		): Promise<unknown> {
			if (disposed || !dispatchFn)
				throw new SandboxError('DISPOSED', 'plugin is stopped');
			const fn = dispatchFn;
			const handles = [
				ctx.newString(kind),
				ctx.newString(id),
				ctx.newString(JSON.stringify(args ?? null)),
			];
			const result = guarded(() =>
				ctx.callFunction(fn, ctx.undefined, ...handles),
			);
			handles.forEach((h) => h.dispose());
			if (isFail(result)) throw failure(result.error);
			return settle(result.value);
		},
		dispose() {
			if (disposed) return;
			disposed = true;
			dispatchFn?.dispose();
			try {
				ctx.dispose();
				runtime.dispose();
			} catch {
				// A runtime that hit its memory limit may refuse a clean teardown; the Worker is dropped anyway.
			}
		},
	};
};

export type Sandbox = ReturnType<typeof createSandbox>;
