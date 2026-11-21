import type { RunnerSession } from '@/runner/api';
import {
	agentFinishedToasts,
	limitKey,
	limitToasts,
	resetText,
} from '../toastTriggers';

jest.mock('@/utils/desktop', () => ({ isDesktopApp: () => false }));
jest.mock('@/utils/dnd', () => ({ isDndActive: () => false }));

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
			fiveHour: { pct: 96, resetsAt: 1_000 + 18 * 60 },
			week: { pct: 94, resetsAt: 9_000 },
		};
		const first = limitToasts(usage, seen, 1_000);
		expect(first).toHaveLength(1);
		expect(first[0].id).toBe(`limit:${limitKey('fiveHour', 1_000 + 18 * 60)}`);
		expect(first[0].body).toBe('5h window resets in 18 min.');
		expect(limitToasts(usage, seen, 1_100)).toHaveLength(0);
		const nextWindow = { fiveHour: { pct: 97, resetsAt: 30_000 } };
		expect(limitToasts(nextWindow, seen, 1_200)).toHaveLength(1);
		expect(limitToasts(undefined, seen, 0)).toEqual([]);
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
