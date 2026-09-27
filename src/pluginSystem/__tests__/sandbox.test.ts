import variant from '@jitl/quickjs-wasmfile-release-sync';
import {
	newQuickJSWASMModuleFromVariant,
	type QuickJSWASMModule,
} from 'quickjs-emscripten-core';
import { PluginError } from '../broker';
import { createSandbox, SandboxError } from '../sandbox';

let quickjs: QuickJSWASMModule;
beforeAll(async () => {
	// jest runs CommonJS without dynamic import(); load the same emscripten module with require.
	quickjs = await newQuickJSWASMModuleFromVariant({
		...variant,
		importModuleLoader: async () =>
			require('@jitl/quickjs-wasmfile-release-sync/emscripten-module'),
	});
});

const start = async (
	code: string,
	call = async (_m: string, _p: unknown): Promise<unknown> => null,
) => {
	const calls: [string, unknown][] = [];
	const sandbox = createSandbox({
		quickjs,
		code,
		cpuMs: 200,
		call: (method, params) => {
			calls.push([method, params]);
			return call(method, params);
		},
	});
	await sandbox.start();
	return { sandbox, calls };
};

it('runs plugin handlers that call the host through tmgr', async () => {
	const { sandbox, calls } = await start(
		`tmgr.commands.register('tmgr.t.go', async (args) => {
			const tasks = await tmgr.tasks.list({ statusId: args.statusId });
			return { count: tasks.length, first: tasks[0].title };
		});`,
		async (method) =>
			method === 'tasks.list' ? [{ title: 'A' }, { title: 'B' }] : null,
	);
	expect(
		await sandbox.dispatch('command', 'tmgr.t.go', { statusId: 3 }),
	).toEqual({ count: 2, first: 'A' });
	expect(calls).toEqual([
		['register', { kind: 'command', id: 'tmgr.t.go' }],
		['tasks.list', { statusId: 3 }],
	]);
	sandbox.dispose();
});

it('has no network, wasm or host internals in the guest', async () => {
	const { sandbox } = await start(
		`tmgr.commands.register('tmgr.t.probe', () => [typeof fetch, typeof XMLHttpRequest, typeof WebAssembly,
			typeof __host, typeof __dispatch, typeof require, typeof process].join(','));`,
	);
	expect(await sandbox.dispatch('command', 'tmgr.t.probe', null)).toBe(
		'undefined,undefined,undefined,undefined,undefined,undefined,undefined',
	);
	sandbox.dispose();
});

it('stops an endless handler and keeps working afterwards', async () => {
	const { sandbox } = await start(
		`tmgr.commands.register('tmgr.t.spin', () => { while (true) {} });
		 tmgr.commands.register('tmgr.t.ok', () => 'still alive');`,
	);
	const started = Date.now();
	await expect(
		sandbox.dispatch('command', 'tmgr.t.spin', null),
	).rejects.toMatchObject({ code: 'TIMEOUT' });
	expect(Date.now() - started).toBeLessThan(2000);
	expect(await sandbox.dispatch('command', 'tmgr.t.ok', null)).toBe(
		'still alive',
	);
	sandbox.dispose();
});

it('stops an endless top-level script', async () => {
	await expect(start('while (true) {}')).rejects.toMatchObject({
		code: 'TIMEOUT',
	});
});

it('ends a memory bomb with an error instead of taking the app down', async () => {
	const { sandbox } = await start(
		`tmgr.commands.register('tmgr.t.bomb', () => { const a = []; for (;;) a.push(new Array(100000).fill(1)); });`,
	);
	await expect(
		sandbox.dispatch('command', 'tmgr.t.bomb', null),
	).rejects.toBeInstanceOf(SandboxError);
	sandbox.dispose();
});

it('hands broker refusals to the plugin with their code', async () => {
	const { sandbox } = await start(
		`tmgr.commands.register('tmgr.t.write', async () => {
			try { await tmgr.tasks.update(1, { title: 'x' }); return 'written'; }
			catch (e) { return e.name + ': ' + e.message; }
		});`,
		async (method) => {
			if (method === 'tasks.update')
				throw new PluginError(
					'PERMISSION_DENIED',
					'tasks.update needs tasks:write',
				);
			return null;
		},
	);
	expect(await sandbox.dispatch('command', 'tmgr.t.write', null)).toBe(
		'PERMISSION_DENIED: tasks.update needs tasks:write',
	);
	sandbox.dispose();
});

it('delivers an event to every handler registered for it', async () => {
	const seen: unknown[] = [];
	const { sandbox } = await start(
		`tmgr.events.on('timer.stopped', (e) => tmgr.storage.set('a', e.taskId));
		 tmgr.events.on('timer.stopped', (e) => tmgr.storage.set('b', e.taskId));`,
		async (method, params) => {
			if (method === 'storage.set') seen.push(params);
			return null;
		},
	);
	await sandbox.dispatch('event', 'timer.stopped', { taskId: 9 });
	expect(seen).toEqual([
		{ key: 'a', value: 9 },
		{ key: 'b', value: 9 },
	]);
	sandbox.dispose();
});

it('does not keep a handler whose registration the host refused', async () => {
	const { sandbox } = await start(
		`tmgr.commands.register('tmgr.t.blocked', () => 'ran').catch(() => {});
		 tmgr.commands.register('tmgr.t.ok', () => 'fine');`,
		async (method, params) => {
			if (method === 'register' && (params as any).id === 'tmgr.t.blocked') {
				throw new PluginError('NOT_DECLARED', 'not declared');
			}
			return null;
		},
	);
	await expect(
		sandbox.dispatch('command', 'tmgr.t.blocked', null),
	).rejects.toBeInstanceOf(SandboxError);
	expect(await sandbox.dispatch('command', 'tmgr.t.ok', null)).toBe('fine');
	sandbox.dispose();
});

it('rejects a dispatch to something the plugin never registered', async () => {
	const { sandbox } = await start('');
	await expect(
		sandbox.dispatch('command', 'tmgr.t.none', null),
	).rejects.toBeInstanceOf(SandboxError);
	sandbox.dispose();
});

it('stops a handler that spins after an await, without failing the next call', async () => {
	const { sandbox } = await start(
		`tmgr.commands.register('tmgr.t.late', async () => { await tmgr.workspace.current(); while (true) {} });
		 tmgr.commands.register('tmgr.t.ok', () => 'fine');`,
	);
	await expect(
		sandbox.dispatch('command', 'tmgr.t.late', null),
	).rejects.toMatchObject({ code: 'TIMEOUT' });
	expect(await sandbox.dispatch('command', 'tmgr.t.ok', null)).toBe('fine');
	sandbox.dispose();
});

it('keeps big payloads and long errors inside the sandbox', async () => {
	const { sandbox, calls } = await start(
		`tmgr.commands.register('tmgr.t.bigcall', () => tmgr.storage.set('k', 'x'.repeat(1100000)).then(() => 'sent', (e) => e.name));
		 tmgr.commands.register('tmgr.t.bigresult', () => 'x'.repeat(2200000));
		 tmgr.commands.register('tmgr.t.longerror', () => { throw new Error('x'.repeat(100000)); });`,
	);
	expect(await sandbox.dispatch('command', 'tmgr.t.bigcall', null)).toBe(
		'RangeError',
	);
	expect(calls.filter(([method]) => method === 'storage.set')).toEqual([]);
	await expect(
		sandbox.dispatch('command', 'tmgr.t.bigresult', null),
	).rejects.toMatchObject({
		code: 'PLUGIN_ERROR',
		message: expect.stringContaining('larger than 2 MB'),
	});
	const error: any = await sandbox
		.dispatch('command', 'tmgr.t.longerror', null)
		.catch((e) => e);
	expect(error.message.length).toBeLessThanOrEqual(1000);
	sandbox.dispose();
});

it('cuts off a loop that runs in the background and keeps answering', async () => {
	let answer: () => void = () => undefined;
	const sandbox = createSandbox({
		quickjs,
		code: `tmgr.workspace.current().then(() => { while (true) {} });
		       tmgr.commands.register('tmgr.t.ok', () => 'still here');`,
		cpuMs: 100,
		call: (method) =>
			method === 'workspace.current'
				? new Promise((resolve) => (answer = () => resolve(null)))
				: Promise.resolve(null),
	});
	await sandbox.start();
	const started = Date.now();
	answer();
	await new Promise((resolve) => setTimeout(resolve, 20));
	expect(await sandbox.dispatch('command', 'tmgr.t.ok', null)).toBe(
		'still here',
	);
	expect(Date.now() - started).toBeLessThan(1000);
	sandbox.dispose();
});

it('gives plugins a fetch-like response for local network calls', async () => {
	const { sandbox, calls } = await start(
		`tmgr.commands.register('tmgr.t.ask', async () => {
			const res = await tmgr.net.fetch('http://localhost:11434/api/generate', { method: 'POST', body: '{"prompt":"hi"}' });
			return [res.status, res.ok, res.headers['content-type'], (await res.json()).response];
		});`,
		async (method) =>
			method === 'net.fetch'
				? {
						status: 200,
						headers: [['Content-Type', 'application/json']],
						body: '{"response":"hello"}',
				  }
				: null,
	);
	expect(await sandbox.dispatch('command', 'tmgr.t.ask', null)).toEqual([
		200,
		true,
		'application/json',
		'hello',
	]);
	expect(calls[1]).toEqual([
		'net.fetch',
		{
			url: 'http://localhost:11434/api/generate',
			method: 'POST',
			body: '{"prompt":"hi"}',
		},
	]);
	sandbox.dispose();
});
