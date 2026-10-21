import type { AgentWorkRun } from '@/actions/tmgr/agentWork';
import {
	agentLabel,
	commitGroups,
	commitUrl,
	formatDeploy,
	formatRelease,
	formatWorkDuration,
	isHttpUrl,
	liveSeconds,
	liveTotals,
	mergeSnapshot,
	timeRows,
	upsertRun,
} from '../agentWork';

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
	version: 0,
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
		const running = run({
			status: 'running',
			ended_at: null,
			duration_seconds: 60,
		});
		expect(liveSeconds(running, Date.parse('2026-09-26T10:05:00Z'))).toBe(300);
	});

	it('counts on from the server duration when it was received, ignoring the browser clock', () => {
		const running = run({
			status: 'running',
			ended_at: null,
			duration_seconds: 60,
		});
		const receivedAt = Date.parse('2030-01-01T00:00:00Z');
		expect(liveSeconds(running, receivedAt + 15_000, receivedAt)).toBe(75);
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
			{ agent_seconds: 1234, human_seconds: 900, human_timer_running: false },
			Date.parse('2026-09-26T10:10:00Z'),
		),
	).toEqual({ agentSeconds: 2400, humanSeconds: 900 });
});

describe('upsertRun', () => {
	it('replaces a known run and puts a new one first', () => {
		const list = [run({ id: 1 }), run({ id: 2 })];
		expect(
			upsertRun(list, run({ id: 2, summary: 'done' })).map((r) => [
				r.id,
				r.summary,
			]),
		).toEqual([
			[1, null],
			[2, 'done'],
		]);
		expect(upsertRun(list, run({ id: 3 })).map((r) => r.id)).toEqual([3, 1, 2]);
	});
});

it('keeps counting the human time while the task timer runs', () => {
	const fetchedAt = Date.parse('2026-09-26T10:00:00Z');
	expect(
		liveTotals(
			[],
			{ agent_seconds: 0, human_seconds: 900, human_timer_running: true },
			fetchedAt + 60_000,
			{},
			fetchedAt,
		).humanSeconds,
	).toBe(960);
});

it('accepts only http(s) links', () => {
	expect(isHttpUrl('https://github.com/tmgr-dev/backend/pull/148')).toBe(true);
	expect(isHttpUrl('javascript:alert(1)')).toBe(false);
	expect(isHttpUrl('data:text/html,x')).toBe(false);
	expect(isHttpUrl(null)).toBe(false);
});

it('ignores a realtime event older than the run it would replace', () => {
	const list = [run({ id: 1, version: 3, status: 'succeeded' })];
	expect(
		upsertRun(list, run({ id: 1, version: 2, status: 'running' }))[0].status,
	).toBe('succeeded');
});

it('keeps newer realtime state when an older snapshot arrives', () => {
	const current = [
		run({ id: 3, version: 0, status: 'running' }),
		run({ id: 2, version: 4, status: 'succeeded' }),
	];
	const snapshot = [
		run({ id: 2, version: 2, status: 'running' }),
		run({ id: 1, version: 1 }),
	];
	expect(mergeSnapshot(current, snapshot).map((r) => [r.id, r.status])).toEqual(
		[
			[3, 'running'],
			[2, 'succeeded'],
			[1, 'succeeded'],
		],
	);
});

describe('commitUrl', () => {
	it('links a known repo and hex sha', () => {
		expect(commitUrl('acme/app', 'abc1234def')).toBe(
			'https://github.com/acme/app/commit/abc1234def',
		);
	});

	it('has no link without a repo', () => {
		expect(commitUrl(null, 'abc1234')).toBeNull();
		expect(commitUrl(undefined, 'abc1234')).toBeNull();
		expect(commitUrl('', 'abc1234')).toBeNull();
	});

	it('has no link for a non-hex sha', () => {
		expect(commitUrl('acme/app', 'main')).toBeNull();
		expect(commitUrl('acme/app', 'abc/../x')).toBeNull();
		expect(commitUrl('acme/app', '')).toBeNull();
	});

	it('rejects malformed or traversing repos', () => {
		expect(commitUrl('acme/../app', 'abc1234')).toBeNull();
		expect(commitUrl('../app', 'abc1234')).toBeNull();
		expect(commitUrl('acme', 'abc1234')).toBeNull();
		expect(commitUrl('acme/app/extra', 'abc1234')).toBeNull();
		expect(commitUrl('acme/app?x=1', 'abc1234')).toBeNull();
		expect(commitUrl('acme/app#frag', 'abc1234')).toBeNull();
		expect(commitUrl('https://evil.test/a', 'abc1234')).toBeNull();
	});
});

describe('commitGroups', () => {
	it('groups commits per repo and branch', () => {
		const groups = commitGroups([
			run({
				id: 1,
				branch: 'feat/x',
				commits: [
					{
						sha: 'aaaaaaa1',
						message: 'one',
						repo: 'acme/app',
						branch: 'feat/x',
					},
					{
						sha: 'bbbbbbb2',
						message: 'two',
						repo: 'acme/ui',
						branch: 'feat/x',
					},
					{
						sha: 'ccccccc3',
						message: 'three',
						repo: 'acme/app',
						branch: 'feat/x',
					},
				],
			}),
		]);
		expect(groups.map((g) => g.header)).toEqual([
			'acme/app · feat/x',
			'acme/ui · feat/x',
		]);
		expect(groups[0].commits.map((c) => c.short)).toEqual([
			'aaaaaaa',
			'ccccccc',
		]);
		expect(groups[0].commits[0].url).toBe(
			'https://github.com/acme/app/commit/aaaaaaa1',
		);
	});

	it('files commits with an unknown repo under the run branch without links', () => {
		const groups = commitGroups([
			run({
				id: 1,
				branch: 'feat/y',
				commits: [{ sha: 'aaaaaaa1', message: 'one' }],
			}),
		]);
		expect(groups).toHaveLength(1);
		expect(groups[0].header).toBe('feat/y');
		expect(groups[0].commits[0].url).toBeNull();
	});

	it('uses the commit branch over the run branch and merges runs', () => {
		const groups = commitGroups([
			run({
				id: 1,
				branch: 'run-a',
				commits: [
					{ sha: 'aaaaaaa1', message: 'one', repo: 'acme/app', branch: 'b' },
				],
			}),
			run({
				id: 2,
				branch: 'run-b',
				commits: [
					{ sha: 'aaaaaaa1', message: 'one', repo: 'acme/app', branch: 'b' },
					{ sha: 'ddddddd4', message: 'four', repo: 'acme/app', branch: 'b' },
				],
			}),
		]);
		expect(groups).toHaveLength(1);
		expect(groups[0].commits).toHaveLength(2);
	});

	it('has an empty header when neither repo nor branch is known', () => {
		const groups = commitGroups([
			run({ id: 1, commits: [{ sha: 'aaaaaaa1', message: null }] }),
		]);
		expect(groups[0].header).toBe('');
	});

	it('skips runs without commits', () => {
		expect(commitGroups([run({ id: 1 })])).toEqual([]);
	});
});

describe('formatRelease', () => {
	it('shows repo, tag, ci link and a shortened digest', () => {
		const digest = 'sha256:' + 'a'.repeat(64);
		expect(
			formatRelease({
				repo: 'acme/app',
				tag: 'v1.2.3',
				ci_url: 'https://ci.test/run/1',
				artifact: 'img',
				digest,
			}),
		).toEqual({
			label: 'acme/app v1.2.3',
			ciUrl: 'https://ci.test/run/1',
			digest: 'sha256:aaaaa',
			digestTitle: digest,
		});
	});

	it('drops a non-http ci url and missing repo', () => {
		expect(
			formatRelease({
				repo: null,
				tag: 'v1',
				ci_url: 'javascript:alert(1)',
				artifact: null,
				digest: null,
			}),
		).toEqual({ label: 'v1', ciUrl: null, digest: null, digestTitle: null });
	});
});

describe('formatDeploy', () => {
	it('joins env and status and keeps only http links', () => {
		expect(
			formatDeploy({ env: 'prod', status: 'success', url: 'https://tmgr.dev' }),
		).toEqual({ label: 'prod · success', url: 'https://tmgr.dev' });
		expect(
			formatDeploy({ env: 'prod', status: 'failed', url: 'ftp://x' }),
		).toEqual({ label: 'prod · failed', url: null });
	});
});

describe('timeRows', () => {
	it('keeps the old Agents row against an old server', () => {
		expect(timeRows(600, {})).toEqual([
			{ key: 'agents', label: 'Agents', seconds: 600 },
		]);
	});

	it('shows active, wall-clock and waiting, hiding the sum when equal to active', () => {
		expect(
			timeRows(600, {
				activeSeconds: 600,
				wallClockSeconds: 900,
				waitingSeconds: 300,
			}),
		).toEqual([
			{ key: 'active', label: 'Agents active', seconds: 600 },
			{ key: 'wall', label: 'Wall-clock', seconds: 900 },
			{ key: 'waiting', label: 'Waiting between runs', seconds: 300 },
		]);
	});

	it('shows the sum of runs when parallel runs make it differ from active', () => {
		const rows = timeRows(1200, { activeSeconds: 700, wallClockSeconds: 700 });
		expect(rows.map((r) => r.key)).toEqual(['active', 'wall', 'agents']);
		expect(rows[2]).toEqual({
			key: 'agents',
			label: 'Agent time (sum of runs)',
			seconds: 1200,
		});
	});

	it('omits absent fields', () => {
		expect(
			timeRows(0, {
				activeSeconds: 0,
				wallClockSeconds: null,
				waitingSeconds: null,
			}),
		).toEqual([{ key: 'active', label: 'Agents active', seconds: 0 }]);
	});
});
