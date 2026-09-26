import type { QuickJSWASMModule } from 'quickjs-emscripten-core';
import {
	wireError,
	type Endpoint,
	type FromWorker,
	type ToWorker,
} from './protocol';
import { createSandbox, type Sandbox } from './sandbox';

/** The Worker side: owns one plugin's sandbox and forwards its host calls to the app. */
export const runPluginWorker = (
	endpoint: Endpoint<ToWorker, FromWorker>,
	loadQuickJS: () => Promise<QuickJSWASMModule>,
) => {
	let sandbox: Sandbox | null = null;
	let nextCall = 1;
	const MAX_IN_FLIGHT = 32;
	const REJECT_DELAY_MS = 200;
	const waiting = new Map<
		number,
		{ resolve: (value: unknown) => void; reject: (error: unknown) => void }
	>();

	const refuse = (message: string) =>
		new Promise<never>((_, reject) =>
			setTimeout(
				() =>
					reject(Object.assign(new Error(message), { code: 'RATE_LIMITED' })),
				REJECT_DELAY_MS,
			),
		);

	/** Floods stay in the Worker: at most 32 calls reach the app at once, and refusals come back slowly. */
	const call = (method: string, params: unknown) => {
		if (waiting.size >= MAX_IN_FLIGHT)
			return refuse('too many calls in flight');
		return new Promise<unknown>((resolve, reject) => {
			const callId = nextCall++;
			waiting.set(callId, {
				resolve,
				reject: (error: any) =>
					error?.code === 'RATE_LIMITED'
						? setTimeout(() => reject(error), REJECT_DELAY_MS)
						: reject(error),
			});
			endpoint.postMessage({ type: 'call', callId, method, params });
		});
	};

	endpoint.addEventListener('message', async ({ data }) => {
		if (data.type === 'result') {
			const pending = waiting.get(data.callId);
			waiting.delete(data.callId);
			if (!pending) return;
			if ('error' in data)
				pending.reject(
					Object.assign(new Error(data.error.message), {
						code: data.error.code,
					}),
				);
			else pending.resolve(data.value);
			return;
		}
		if (data.type === 'start') {
			try {
				sandbox = createSandbox({
					quickjs: await loadQuickJS(),
					code: data.code,
					call,
					cpuMs: data.cpuMs,
					wallMs: data.wallMs,
					onBackgroundError: (error) =>
						endpoint.postMessage({ type: 'fault', error: wireError(error) }),
				});
				await sandbox.start();
				endpoint.postMessage({ type: 'started' });
			} catch (error) {
				endpoint.postMessage({ type: 'startFailed', error: wireError(error) });
			}
			return;
		}
		if (data.type === 'dispatch') {
			try {
				if (!sandbox)
					throw Object.assign(new Error('plugin is not started'), {
						code: 'DISPOSED',
					});
				const value = await sandbox.dispatch(data.kind, data.target, data.args);
				endpoint.postMessage({
					type: 'dispatched',
					id: data.id,
					ok: true,
					value,
				});
			} catch (error) {
				endpoint.postMessage({
					type: 'dispatched',
					id: data.id,
					ok: false,
					error: wireError(error),
				});
			}
		}
	});
};
