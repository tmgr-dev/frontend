import variant from '@jitl/quickjs-wasmfile-release-sync';
import { MessageChannel } from 'node:worker_threads';
import {
	newQuickJSWASMModuleFromVariant,
	type QuickJSWASMModule,
} from 'quickjs-emscripten-core';
import { PluginError } from '../broker';
import { startPluginProcess, type WorkerEndpoint } from '../process';
import { runPluginWorker } from '../workerRuntime';

let quickjs: Promise<QuickJSWASMModule>;
beforeAll(() => {
	quickjs = newQuickJSWASMModuleFromVariant({
		...variant,
		importModuleLoader: async () =>
			require('@jitl/quickjs-wasmfile-release-sync/emscripten-module'),
	});
});

const pair = () => {
	const { port1, port2 } = new MessageChannel();
	const wrap = (port: typeof port1) => ({
		postMessage: (message: unknown) => port.postMessage(message),
		addEventListener: (
			_: 'message',
			listener: (event: { data: any }) => void,
		) => port.on('message', (data) => listener({ data })),
	});
	const worker = wrap(port2);
	runPluginWorker(worker as any, () => quickjs);
	let terminated = false;
	const endpoint: WorkerEndpoint & { terminated: () => boolean } = {
		...(wrap(port1) as any),
		terminate: () => {
			terminated = true;
			port1.close();
		},
		terminated: () => terminated,
	};
	return endpoint;
};

it('runs a plugin in its worker and answers its calls through the host', async () => {
	const endpoint = pair();
	const calls: unknown[] = [];
	const process = startPluginProcess(
		`tmgr.commands.register('tmgr.p.count', async () => (await tmgr.tasks.list({})).length);`,
		{
			endpoint,
			call: async (method, params) => {
				calls.push([method, params]);
				if (method === 'tasks.list') return [{ id: 1 }, { id: 2 }, { id: 3 }];
				return null;
			},
			onCrash: () => undefined,
		},
	);
	await process.ready;
	expect(await process.dispatch('command', 'tmgr.p.count', null)).toBe(3);
	expect(calls).toEqual([
		['register', { kind: 'command', id: 'tmgr.p.count' }],
		['tasks.list', {}],
	]);
	process.stop();
	expect(endpoint.terminated()).toBe(true);
});

it('passes host refusals back to the plugin', async () => {
	const endpoint = pair();
	const process = startPluginProcess(
		`tmgr.commands.register('tmgr.p.try', () => tmgr.tasks.update(1, {}).then(() => 'ok', (e) => e.name));`,
		{
			endpoint,
			call: async (method) => {
				if (method === 'tasks.update')
					throw new PluginError('PERMISSION_DENIED', 'no');
				return null;
			},
			onCrash: () => undefined,
		},
	);
	await process.ready;
	expect(await process.dispatch('command', 'tmgr.p.try', null)).toBe(
		'PERMISSION_DENIED',
	);
	process.stop();
});

it('reports a plugin that fails to start', async () => {
	const process = startPluginProcess('throw new Error("boom")', {
		endpoint: pair(),
		call: async () => null,
		onCrash: () => undefined,
	});
	await expect(process.ready).rejects.toThrow('boom');
	process.stop();
});

it('kills a worker that stops answering and reports the crash', async () => {
	const crashes: string[] = [];
	const endpoint = {
		postMessage: () => undefined,
		addEventListener: () => undefined,
		terminate: jest.fn(),
	};
	const process = startPluginProcess('', {
		endpoint,
		call: async () => null,
		onCrash: (reason) => crashes.push(reason),
		startMs: 50,
	});
	await expect(process.ready).rejects.toMatchObject({ code: 'CRASHED' });
	expect(endpoint.terminate).toHaveBeenCalled();
	expect(crashes).toEqual(['plugin did not start within 50 ms']);
	await expect(process.dispatch('command', 'x', null)).rejects.toMatchObject({
		code: 'CRASHED',
	});
});
