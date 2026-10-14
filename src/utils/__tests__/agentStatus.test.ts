import {
	formatSince,
	groupByTask,
	replaceSessions,
	sessionTooltip,
	statusTooltip,
	upsertSession,
	type AgentSession,
} from '../agentStatus';

const NOW = Date.parse('2026-10-13T12:00:00Z');

const session = (overrides: Partial<AgentSession> = {}): AgentSession => ({
	session_id: 'sess-0001',
	workspace_id: 5,
	task: { id: 10, key: 'TM-10' },
	state: 'working',
	state_since: '2026-10-13T11:55:00Z',
	started_at: '2026-10-13T11:00:00Z',
	updated_at: '2026-10-13T11:59:00Z',
	ended_at: null,
	agent_profile: 'claude-code',
	host_label: 'mbp',
	user: { id: 1, name: 'Ann' },
	stale: false,
	...overrides,
});

describe('upsertSession', () => {
	it('adds a new session and replaces one with the same id', () => {
		const first = upsertSession([], session());
		expect(first).toHaveLength(1);
		const second = upsertSession(first, session({ state: 'blocked' }));
		expect(second).toHaveLength(1);
		expect(second[0].state).toBe('blocked');
	});

	it('drops a session that ended', () => {
		const list = upsertSession([], session());
		const after = upsertSession(
			list,
			session({ ended_at: '2026-10-13T11:59:30Z', state: 'done' }),
		);
		expect(after).toEqual([]);
	});

	it('ignores an ended session that was never known', () => {
		expect(upsertSession([], session({ ended_at: '2026-10-13T11:00:00Z' }))).toEqual([]);
	});
});

describe('replaceSessions', () => {
	it('keeps only live sessions from a snapshot', () => {
		const result = replaceSessions([
			session({ session_id: 'a' }),
			session({ session_id: 'b', ended_at: '2026-10-13T11:00:00Z' }),
		]);
		expect(result.map((s) => s.session_id)).toEqual(['a']);
	});
});

describe('groupByTask', () => {
	it('shows the most important state with a count', () => {
		const grouped = groupByTask([
			session({ session_id: 'a', state: 'idle' }),
			session({ session_id: 'b', state: 'working' }),
			session({ session_id: 'c', state: 'blocked' }),
			session({ session_id: 'd', state: 'done' }),
		]);
		expect(grouped[10].state).toBe('blocked');
		expect(grouped[10].count).toBe(4);
	});

	it('ranks blocked over working over waiting over idle over done', () => {
		const order = ['done', 'idle', 'waiting', 'working', 'blocked'] as const;
		order.reduce<AgentSession[]>((acc, state, index) => {
			const next = [...acc, session({ session_id: `s${index}`, state })];
			expect(groupByTask(next)[10].state).toBe(state);
			return next;
		}, []);
	});

	it('prefers a fresh session over a stale one of the same state', () => {
		const grouped = groupByTask([
			session({ session_id: 'a', stale: true }),
			session({ session_id: 'b', stale: false }),
		]);
		expect(grouped[10].stale).toBe(false);
	});

	it('is stale when the only session is stale', () => {
		expect(groupByTask([session({ stale: true })])[10].stale).toBe(true);
	});

	it('skips sessions without a task and separates tasks', () => {
		const grouped = groupByTask([
			session({ session_id: 'a', task: null }),
			session({ session_id: 'b', task: { id: 11, key: 'TM-11' } }),
		]);
		expect(Object.keys(grouped)).toEqual(['11']);
	});
});

describe('formatSince', () => {
	it('formats minutes, hours and days', () => {
		expect(formatSince('2026-10-13T11:59:50Z', NOW)).toBe('just now');
		expect(formatSince('2026-10-13T11:55:00Z', NOW)).toBe('5 min ago');
		expect(formatSince('2026-10-13T09:00:00Z', NOW)).toBe('3 h ago');
		expect(formatSince('2026-10-10T12:00:00Z', NOW)).toBe('3 d ago');
		expect(formatSince(null, NOW)).toBe('');
	});
});

describe('tooltip text', () => {
	it('names the agent, host and time', () => {
		expect(sessionTooltip(session(), NOW)).toBe(
			'Working: claude-code on mbp · since 5 min ago',
		);
	});

	it('omits the host when there is none and marks stale sessions', () => {
		expect(sessionTooltip(session({ host_label: null, stale: true }), NOW)).toBe(
			'Working: claude-code · since 5 min ago · no recent updates',
		);
	});

	it('passes server strings through verbatim as plain text', () => {
		const hostile = '<img src=x onerror=alert(1)>';
		const text = sessionTooltip(session({ host_label: hostile }), NOW);
		expect(text).toContain(hostile);
		expect(text).not.toMatch(/&lt;/);
	});

	it('lists every session on its own line', () => {
		const grouped = groupByTask([
			session({ session_id: 'a', state: 'blocked', host_label: 'one' }),
			session({ session_id: 'b', state: 'idle', host_label: 'two' }),
		]);
		const lines = statusTooltip(grouped[10], NOW).split('\n');
		expect(lines).toHaveLength(2);
		expect(lines[0]).toContain('Blocked');
		expect(lines[1]).toContain('Idle');
	});
});
