import {
	agentLabel,
	formatWorkDuration,
	liveSeconds,
	liveTotals,
	upsertRun,
} from '../agentWork';
import type { AgentWorkRun } from '@/actions/tmgr/agentWork';

const run = (overrides: Partial<AgentWorkRun> = {}): AgentWorkRun => ({
	id: 1,
	task_id: 42,
	workspace_id: 5,
	user_id: 7,
	agent: 'claude-code',
	model: null,
	session_id: null,
	branch: null,
	status: 'succeeded',
	started_at: '2026-09-26T10:00:00Z',
	ended_at: '2026-09-26T10:30:00Z',
	duration_seconds: 1800,
	summary: null,
	pr_url: null,
	commits: [],
	tests: null,
	...overrides,
});

describe('formatWorkDuration', () => {
	it('keeps the two most significant units', () => {
		expect(formatWorkDuration(0)).toBe('0s');
		expect(formatWorkDuration(45)).toBe('45s');
		expect(formatWorkDuration(125)).toBe('2m 5s');
		expect(formatWorkDuration(3600 + 20 * 60 + 9)).toBe('1h 20m');
	});
});

describe('agentLabel', () => {
	it('names known agents and prettifies the rest', () => {
		expect(agentLabel('claude-code')).toBe('Claude Code');
		expect(agentLabel('codex')).toBe('Codex');
		expect(agentLabel('my-bot')).toBe('My Bot');
	});
});

describe('liveSeconds', () => {
	it('uses the stored duration of a finished run', () => {
		expect(liveSeconds(run(), Date.parse('2026-09-26T12:00:00Z'))).toBe(1800);
	});

	it('counts a running run up to now', () => {
		const running = run({ status: 'running', ended_at: null, duration_seconds: 60 });
		expect(liveSeconds(running, Date.parse('2026-09-26T10:05:00Z'))).toBe(300);
	});
});

it('adds running runs up to now to the agent total and leaves the human time alone', () => {
	const runs = [
		run(),
		run({ id: 2, status: 'running', ended_at: null, duration_seconds: 0 }),
	];
	expect(
		liveTotals(
			runs,
			{ agent_seconds: 1234, human_seconds: 900 },
			Date.parse('2026-09-26T10:10:00Z'),
		),
	).toEqual({ agentSeconds: 2400, humanSeconds: 900 });
});

describe('upsertRun', () => {
	it('replaces a known run and puts a new one first', () => {
		const list = [run({ id: 1 }), run({ id: 2 })];
		expect(upsertRun(list, run({ id: 2, summary: 'done' })).map((r) => [r.id, r.summary])).toEqual([
			[1, null],
			[2, 'done'],
		]);
		expect(upsertRun(list, run({ id: 3 })).map((r) => r.id)).toEqual([3, 1, 2]);
	});
});
