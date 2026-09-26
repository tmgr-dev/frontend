import type { DispatchKind } from './sandbox';

/** Host calls that wait for the user (a file dialog): that time does not count against deadlines. */
export const USER_WAIT_METHODS = new Set(['files.pick']);
export const MAX_USER_WAIT_MS = 5 * 60_000;

export interface WireError {
	code: string;
	message: string;
}

export type ToWorker =
	| { type: 'start'; code: string; cpuMs: number; wallMs: number }
	| {
			type: 'dispatch';
			id: number;
			kind: DispatchKind;
			target: string;
			args: unknown;
	  }
	| { type: 'result'; callId: number; ok: true; value: unknown }
	| { type: 'result'; callId: number; ok: false; error: WireError };

export type FromWorker =
	| { type: 'started' }
	| { type: 'fault'; error: WireError }
	| { type: 'startFailed'; error: WireError }
	| { type: 'call'; callId: number; method: string; params: unknown }
	| { type: 'dispatched'; id: number; ok: true; value: unknown }
	| { type: 'dispatched'; id: number; ok: false; error: WireError };

export interface Endpoint<In, Out> {
	postMessage(message: Out): void;
	addEventListener(
		type: 'message',
		listener: (event: { data: In }) => void,
	): void;
}

export const wireError = (error: unknown): WireError => ({
	code:
		typeof (error as any)?.code === 'string'
			? (error as any).code
			: 'PLUGIN_ERROR',
	message: (error instanceof Error ? error.message : String(error)).slice(
		0,
		1000,
	),
});
