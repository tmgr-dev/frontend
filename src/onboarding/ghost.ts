import { DEMO_AGENT, type DemoCard } from '@/local/demoSeed';
import { GHOST_HEADER } from '@/local/ghostHeader';
import type { AxiosInstance } from 'axios';

export type GhostPhase = 'idle' | 'playing' | 'finished' | 'aborted';
export type GhostOutcome = 'completed' | 'skipped' | 'aborted' | 'failed';

export interface GhostClock {
	now(): number;
	setTimeout(fn: () => void, ms: number): unknown;
	clearTimeout(handle: unknown): void;
}

export interface GhostEnv {
	request(method: string, url: string, body?: unknown): Promise<any>;
	taskChanged(taskId: number): void;
	runChanged(run: any): void;
	timerChanged(): void;
}

export interface GhostStep {
	atMs: number;
	run(env: GhostEnv): Promise<void>;
}

const realClock: GhostClock = {
	now: () => Date.now(),
	setTimeout: (fn, ms) => setTimeout(fn, ms),
	clearTimeout: (handle) => clearTimeout(handle as number),
};

export const createGhostPlayer = (options: {
	steps: GhostStep[];
	clock?: GhostClock;
	env?: GhostEnv;
	onError?: (error: unknown) => void;
}) => {
	const clock = options.clock ?? realClock;
	let phase: GhostPhase = 'idle';
	let fast = false;
	let aborted = false;
	let wake: (() => void) | null = null;
	let timer: unknown = null;
	let running: Promise<GhostOutcome> | null = null;

	const sleep = (ms: number) =>
		new Promise<void>((resolve) => {
			if (fast || aborted || ms <= 0) return resolve();
			wake = resolve;
			timer = clock.setTimeout(() => {
				wake = null;
				timer = null;
				resolve();
			}, ms);
		});

	const interrupt = () => {
		if (timer !== null) clock.clearTimeout(timer);
		timer = null;
		const resolve = wake;
		wake = null;
		resolve?.();
	};

	const play = async (): Promise<GhostOutcome> => {
		phase = 'playing';
		const startedAt = clock.now();
		try {
			for (const step of options.steps) {
				await sleep(step.atMs - (clock.now() - startedAt));
				if (aborted) break;
				await step.run(options.env as GhostEnv);
			}
		} catch (error) {
			phase = 'aborted';
			options.onError?.(error);
			return 'failed';
		}
		if (aborted) {
			phase = 'aborted';
			return 'aborted';
		}
		phase = 'finished';
		return fast ? 'skipped' : 'completed';
	};

	return {
		phase: () => phase,
		start: () => (running ??= play()),
		skip: () => {
			if (phase !== 'playing') return;
			fast = true;
			interrupt();
		},
		abort: () => {
			if (phase !== 'playing') return;
			aborted = true;
			interrupt();
		},
	};
};

const REDIRECT_SUMMARY =
	'Fixed the redirect after sign-in; kept the requested page in the URL.';

export const buildScenario = (card: DemoCard): GhostStep[] => {
	let runId = 0;
	const id = card.taskId;
	const moveTo =
		(status: number): GhostStep['run'] =>
		async (env) => {
			await env.request('PUT', `tasks/${id}/${status}`);
			env.taskChanged(id);
		};
	const progress =
		(body: Record<string, unknown>): GhostStep['run'] =>
		async (env) => {
			env.runChanged(await env.request('PATCH', `agent-work/${runId}`, body));
		};
	return [
		{ atMs: 800, run: moveTo(card.inProgress) },
		{
			atMs: 2200,
			run: async (env) => {
				await env.request('POST', `tasks/${id}/countdown`);
				env.taskChanged(id);
				env.timerChanged();
			},
		},
		{
			atMs: 3200,
			run: async (env) => {
				const run = await env.request('POST', `tasks/${id}/agent-work`, {
					agent: DEMO_AGENT,
					branch: 'fix/login-redirect',
				});
				runId = run.id;
				env.runChanged(run);
			},
		},
		{
			atMs: 6500,
			run: progress({
				summary: 'Reading src/auth/redirect.ts and the login handler.',
			}),
		},
		{
			atMs: 10500,
			run: progress({
				summary: 'Keeping the return path in the URL after sign-in.',
				commits: [
					{
						sha: 'a1b2c3d',
						message: 'fix: keep the return path after sign-in',
					},
				],
			}),
		},
		{
			atMs: 15000,
			run: progress({
				tests: { passed: 14, failed: 0, command: 'npm test' },
			}),
		},
		{
			atMs: 18500,
			run: async (env) => {
				await env.request('DELETE', `tasks/${id}/countdown`);
				await env.request('PUT', `tasks/${id}/time`, { common_time: 0 });
				env.taskChanged(id);
				env.timerChanged();
			},
		},
		{
			atMs: 20000,
			run: async (env) => {
				env.runChanged(
					await env.request('POST', `agent-work/${runId}/finish`, {
						status: 'succeeded',
						summary: REDIRECT_SUMMARY,
					}),
				);
			},
		},
		{ atMs: 22500, run: moveTo(card.review) },
	];
};

export const createAxiosGhostEnv = (
	http: Pick<AxiosInstance, 'request'>,
	hooks: {
		workspaceId: number;
		taskChanged: (taskId: number) => void;
		runChanged: (run: any) => void;
		timerChanged: () => void;
	},
): GhostEnv => ({
	async request(method, url, body) {
		const response = await http.request({
			method,
			url,
			data: body,
			params: { workspace_id: hooks.workspaceId },
			headers: { [GHOST_HEADER]: '1' },
		});
		return response.data?.data ?? response.data;
	},
	taskChanged: hooks.taskChanged,
	runChanged: hooks.runChanged,
	timerChanged: hooks.timerChanged,
});

type PlayListener = () => void;
const playListeners = new Set<PlayListener>();

export const requestGhostPlay = (): void =>
	playListeners.forEach((listener) => listener());

export const onGhostPlayRequested = (listener: PlayListener): (() => void) => {
	playListeners.add(listener);
	return () => playListeners.delete(listener);
};

let stopHandler: (() => Promise<void>) | null = null;

export const registerGhostStop = (
	handler: () => Promise<void>,
): (() => void) => {
	stopHandler = handler;
	return () => {
		if (stopHandler === handler) stopHandler = null;
	};
};

export const stopGhost = async (): Promise<void> => {
	await stopHandler?.();
};
