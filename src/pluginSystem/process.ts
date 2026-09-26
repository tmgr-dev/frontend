import {
	wireError,
	type Endpoint,
	type FromWorker,
	type ToWorker,
} from './protocol';
import type { DispatchKind } from './sandbox';

export interface WorkerEndpoint extends Endpoint<FromWorker, ToWorker> {
	terminate(): void;
}

export interface PluginProcessOptions {
	endpoint: WorkerEndpoint;
	call: (method: string, params: unknown) => Promise<unknown>;
	onCrash: (reason: string) => void;
	/** Plugin code failed in the background, outside any call from the app. */
	onFault?: (reason: string) => void;
	cpuMs?: number;
	wallMs?: number;
	startMs?: number;
}

export class DispatchError extends Error {
	constructor(readonly code: string, message: string) {
		super(message);
	}
}

/**
 * The app side of one plugin Worker. The Worker is the plugin's identity: the host created it for this
 * plugin, so every call arriving on it is this plugin's call. A Worker that stops answering is killed.
 */
export const startPluginProcess = (
	code: string,
	options: PluginProcessOptions,
) => {
	const { endpoint, cpuMs = 500, wallMs = 15_000, startMs = 10_000 } = options;
	let stopped = false;
	let nextDispatch = 1;
	const pending = new Map<
		number,
		{ resolve: (v: unknown) => void; reject: (e: unknown) => void }
	>();
	let started: { resolve: () => void; reject: (e: unknown) => void } | null =
		null;

	const kill = (reason: string) => {
		if (stopped) return;
		stopped = true;
		endpoint.terminate();
		const error = new DispatchError('CRASHED', reason);
		pending.forEach(({ reject }) => reject(error));
		pending.clear();
		started?.reject(error);
		options.onCrash(reason);
	};

	endpoint.addEventListener('message', ({ data }) => {
		if (stopped) return;
		switch (data.type) {
			case 'started':
				return started?.resolve();
			case 'fault':
				return options.onFault?.(`${data.error.code}: ${data.error.message}`);
			case 'startFailed':
				return started?.reject(
					new DispatchError(data.error.code, data.error.message),
				);
			case 'call': {
				const answer = (message: ToWorker) => {
					if (stopped) return;
					try {
						endpoint.postMessage(message);
					} catch (error) {
						// A value the Worker cannot receive must still answer the call, or the plugin waits forever.
						endpoint.postMessage({
							type: 'result',
							callId: data.callId,
							ok: false,
							error: wireError(
								Object.assign(
									new Error(`host answer could not be sent: ${error}`),
									{ code: 'HOST_ERROR' },
								),
							),
						});
					}
				};
				options.call(data.method, data.params).then(
					(value) =>
						answer({ type: 'result', callId: data.callId, ok: true, value }),
					(error) =>
						answer({
							type: 'result',
							callId: data.callId,
							ok: false,
							error: wireError(error),
						}),
				);
				return;
			}
			case 'dispatched': {
				const waiter = pending.get(data.id);
				pending.delete(data.id);
				if (!waiter) return;
				if ('error' in data)
					waiter.reject(new DispatchError(data.error.code, data.error.message));
				else waiter.resolve(data.value);
			}
		}
	});

	let startTimer: ReturnType<typeof setTimeout> | undefined;
	const ready = new Promise<void>((resolve, reject) => {
		started = {
			resolve: () => (clearTimeout(startTimer), resolve()),
			reject: (error) => (clearTimeout(startTimer), reject(error)),
		};
		startTimer = setTimeout(
			() => kill(`plugin did not start within ${startMs} ms`),
			startMs,
		);
		endpoint.postMessage({ type: 'start', code, cpuMs, wallMs });
	});

	return {
		ready,
		dispatch(
			kind: DispatchKind,
			target: string,
			args: unknown,
		): Promise<unknown> {
			if (stopped)
				return Promise.reject(
					new DispatchError('CRASHED', 'plugin is stopped'),
				);
			const id = nextDispatch++;
			return new Promise((resolve, reject) => {
				// The sandbox gives up after wallMs itself; a Worker silent past that is stuck, not slow.
				const timer = setTimeout(
					() => kill(`plugin did not answer ${kind} ${target}`),
					wallMs + 2_000,
				);
				pending.set(id, {
					resolve: (value) => (clearTimeout(timer), resolve(value)),
					reject: (error) => (clearTimeout(timer), reject(error)),
				});
				endpoint.postMessage({ type: 'dispatch', id, kind, target, args });
			});
		},
		stop() {
			if (stopped) return;
			stopped = true;
			endpoint.terminate();
			started?.reject(new DispatchError('STOPPED', 'plugin was stopped'));
			pending.forEach(({ reject }) =>
				reject(new DispatchError('STOPPED', 'plugin was stopped')),
			);
			pending.clear();
		},
		get alive() {
			return !stopped;
		},
	};
};

export type PluginProcess = ReturnType<typeof startPluginProcess>;
