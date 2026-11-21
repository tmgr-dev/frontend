import type { RunnerSession } from '@/runner/api';
import {
	agentFinishedToasts,
	limitKey,
	limitToasts,
	pushToast,
	resetText,
} from '../toastTriggers';

const mockEnv = { desktop: false, toasts: true, dnd: false };
const mockInvoke = jest.fn();

jest.mock('@/utils/desktop', () => ({
	isDesktopApp: () => mockEnv.desktop,
	supportsToastWindow: () => mockEnv.toasts,
}));
jest.mock('@/utils/dnd', () => ({ isDndActive: () => mockEnv.dnd }));
jest.mock('@tauri-apps/api/core', () => ({
	invoke: (...args: unknown[]) => mockInvoke(...args),
}));

const session = (
	id: string,
	kind: RunnerSession['state']['kind'],
	detail: RunnerSession['state']['detail'] = null,
	task: RunnerSession['task'] = { workspaceId: 5, taskId: 7 },
): RunnerSession => ({
	id,
	name: `run ${id}`,
	profile: null,
	task,
	state: { kind, sinceMs: 0, detail },
	createdMs: 0,
});

describe('limit toasts', () => {
	it('pushes one toast per window and reset time', () => {
		const seen = new Set<string>();
		const usage = {
			providers: [
				{
					tool: 'claude' as const,
					fiveHour: { pct: 96, resetsAt: 1_000 + 18 * 60 },
					week: { pct: 94, resetsAt: 9_000 },
				},
			],
		};
		const first = limitToasts(usage, seen, 1_000);
		expect(first).toHaveLength(1);
		expect(first[0].key).toBe(limitKey('fiveHour', 1_000 + 18 * 60));
		expect(first[0].toast.id).toBe(`limit:${first[0].key}`);
		expect(first[0].toast.body).toBe('5h window resets in 18 min.');
		seen.add(first[0].key);
		expect(limitToasts(usage, seen, 1_100)).toHaveLength(0);
		const nextWindow = {
			providers: [
				{ tool: 'claude' as const, fiveHour: { pct: 97, resetsAt: 30_000 } },
			],
		};
		expect(limitToasts(nextWindow, seen, 1_200)).toHaveLength(1);
		expect(limitToasts(undefined, seen, 0)).toEqual([]);
	});

	it('keeps offering a limit until it is remembered', () => {
		const usage = {
			providers: [
				{ tool: 'claude' as const, fiveHour: { pct: 99, resetsAt: 5_000 } },
			],
		};
		const seen = new Set<string>();
		expect(limitToasts(usage, seen, 1_000)).toHaveLength(1);
		expect(limitToasts(usage, seen, 1_060)).toHaveLength(1);
		expect(seen.size).toBe(0);
	});

	it('reads the reset time', () => {
		expect(resetText(null, 0)).toBe('resets soon');
		expect(resetText(30, 0)).toBe('resets in 1 min');
		expect(resetText(3 * 3600, 0)).toBe('resets in 3 h');
	});
});

describe('agent finished toasts', () => {
	it('fires only on a running session turning exited with done', () => {
		const before = new Map([
			['a', 'working'],
			['b', 'working'],
			['c', 'exited'],
		]);
		const { toasts, next } = agentFinishedToasts(
			before,
			[
				session('a', 'exited', 'done'),
				session('b', 'exited', 'failed'),
				session('c', 'exited', 'done'),
				session('d', 'exited', 'done'),
			],
			2 * 3_600_000 + 5 * 60_000,
		);
		expect(toasts.map((t) => t.id)).toEqual(['agent:a']);
		expect(toasts[0].meta).toBe('2h 05m');
		expect(toasts[0].data).toEqual({ workspaceId: 5, taskId: 7 });
		expect(toasts[0].actions[0].id).toBe('open-task');
		expect(next.get('d')).toBe('exited');
	});

	it('skips sessions without a task', () => {
		const { toasts } = agentFinishedToasts(
			new Map([['a', 'working']]),
			[session('a', 'exited', 'done', null)],
			0,
		);
		expect(toasts).toEqual([]);
	});
});

describe('pushToast', () => {
	const toast = {
		id: 'a',
		kind: 'agent' as const,
		title: 't',
		body: 'b',
		actions: [],
	};

	beforeEach(() => {
		Object.assign(mockEnv, { desktop: true, toasts: true, dnd: false });
		mockInvoke.mockReset();
		mockInvoke.mockResolvedValue(undefined);
	});

	afterAll(() => {
		Object.assign(mockEnv, { desktop: false, toasts: true, dnd: false });
	});

	it('reports true only when the toast was handed to the app', async () => {
		await expect(pushToast(toast)).resolves.toBe(true);
		expect(mockInvoke).toHaveBeenCalledWith('toast_push', { toast });
	});

	it('drops the toast under do-not-disturb, outside the desktop app and without a toast window', async () => {
		mockEnv.dnd = true;
		await expect(pushToast(toast)).resolves.toBe(false);
		Object.assign(mockEnv, { dnd: false, toasts: false });
		await expect(pushToast(toast)).resolves.toBe(false);
		Object.assign(mockEnv, { toasts: true, desktop: false });
		await expect(pushToast(toast)).resolves.toBe(false);
		expect(mockInvoke).not.toHaveBeenCalled();
	});

	it('reports false when the command fails', async () => {
		jest.spyOn(console, 'error').mockImplementation(() => undefined);
		mockInvoke.mockRejectedValue(new Error('no'));
		await expect(pushToast(toast)).resolves.toBe(false);
	});
});
