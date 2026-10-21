import {
	actionOutcome,
	expiryHint,
	hasOpenConfirmation,
	isLongBody,
	normalizeAgentMessage,
	normalizePendingAction,
	openActionIds,
	safeResultUrl,
} from '../agentConfirmation';

const NOW = Date.parse('2026-10-21T10:00:00Z');
const iso = (offsetMs: number) => new Date(NOW + offsetMs).toISOString();

const action = (over: Record<string, unknown> = {}) =>
	normalizePendingAction({
		id: 1,
		tool: 'create_task',
		risk: 'low',
		status: 'pending',
		preview: { title: 'Create task', fields: [], body: null },
		result: null,
		expires_at: iso(14 * 60000),
		...over,
	});

describe('normalizePendingAction', () => {
	it('lower-cases risk and status and accepts tool_name', () => {
		const a = normalizePendingAction({
			id: 3,
			tool_name: 'add_comment',
			risk: 'HIGH',
			status: 'PENDING',
			preview: { title: 'T', fields: [{ label: 'a', value: 'b' }], body: 'x' },
			expires_at: 'z',
		});
		expect(a.tool).toBe('add_comment');
		expect(a.risk).toBe('high');
		expect(a.status).toBe('pending');
		expect(a.preview.fields).toEqual([{ label: 'a', value: 'b' }]);
		expect(a.result).toBeNull();
	});

	it('falls back safely on missing parts', () => {
		const a = normalizePendingAction({ id: 4 });
		expect(a.risk).toBe('low');
		expect(a.tool).toBe('');
		expect(a.preview).toEqual({ title: '', fields: [], body: null });
		expect(a.status).toBe('pending');
	});

	it('turns an object of fields into label/value pairs', () => {
		const a = normalizePendingAction({
			id: 5,
			preview: { title: 'T', fields: { Title: 'x', Count: 2 } },
		});
		expect(a.preview.fields).toEqual([
			{ label: 'Title', value: 'x' },
			{ label: 'Count', value: '2' },
		]);
	});
});

describe('normalizeAgentMessage', () => {
	it('leaves messages without actions untouched', () => {
		const m = { id: 1, role: 'user', content: 'x', status: 'done', steps: [] };
		expect(normalizeAgentMessage(m as never)).toBe(m);
	});

	it('normalizes pending_actions', () => {
		const m = normalizeAgentMessage({
			id: 1,
			status: 'awaiting',
			pending_actions: [{ id: 2, risk: 'HIGH' }],
		} as never);
		expect(m.pending_actions?.[0].risk).toBe('high');
	});
});

describe('openActionIds / hasOpenConfirmation', () => {
	it('collects only pending, unexpired ids', () => {
		const actions = [
			action({ id: 1 }),
			action({ id: 2, status: 'executed' }),
			action({ id: 3, expires_at: iso(-1000) }),
			action({ id: 4 }),
		];
		expect(openActionIds(actions, NOW)).toEqual([1, 4]);
	});

	it('is true only for an awaiting message with an open action', () => {
		const awaiting = {
			status: 'awaiting',
			pending_actions: [action()],
		} as never;
		const decided = {
			status: 'running',
			pending_actions: [action({ status: 'executing' })],
		} as never;
		expect(hasOpenConfirmation([awaiting], NOW)).toBe(true);
		expect(hasOpenConfirmation([decided], NOW)).toBe(false);
		expect(hasOpenConfirmation([], NOW)).toBe(false);
	});
});

describe('expiryHint', () => {
	it('shows minutes left rounded up', () => {
		expect(expiryHint(iso(14 * 60000), NOW)).toBe('Expires in 14 min');
		expect(expiryHint(iso(14 * 60000 - 5000), NOW)).toBe('Expires in 14 min');
	});

	it('shows a short hint under a minute', () => {
		expect(expiryHint(iso(20000), NOW)).toBe('Expires in less than a minute');
	});

	it('shows the expired text once passed', () => {
		expect(expiryHint(iso(-1), NOW)).toBe('Expired — nothing was changed');
	});

	it('returns empty for an invalid date', () => {
		expect(expiryHint('nope', NOW)).toBe('');
	});
});

describe('actionOutcome', () => {
	it('has no outcome for an open action', () => {
		expect(actionOutcome(action(), NOW)).toBeNull();
	});

	it('reports done with a link', () => {
		expect(
			actionOutcome(
				action({ status: 'executed', result: { url: 'https://tmgr.dev/a' } }),
				NOW,
			),
		).toEqual({ text: 'Done', tone: 'ok', url: 'https://tmgr.dev/a' });
	});

	it('drops unsafe links', () => {
		expect(
			actionOutcome(
				action({ status: 'executed', result: { url: 'javascript:alert(1)' } }),
				NOW,
			),
		).toEqual({ text: 'Done', tone: 'ok', url: null });
	});

	it('reports failures with the message', () => {
		expect(
			actionOutcome(
				action({ status: 'failed', result: { message: 'No access' } }),
				NOW,
			)?.text,
		).toBe('Failed: No access');
		expect(actionOutcome(action({ status: 'failed' }), NOW)?.text).toBe(
			'Failed',
		);
	});

	it('maps the remaining statuses', () => {
		expect(actionOutcome(action({ status: 'cancelled' }), NOW)?.text).toBe(
			'Cancelled',
		);
		expect(actionOutcome(action({ status: 'expired' }), NOW)?.text).toBe(
			'Expired — nothing was changed',
		);
		expect(actionOutcome(action({ status: 'superseded' }), NOW)?.text).toBe(
			'Skipped — you sent a new message',
		);
		expect(actionOutcome(action({ status: 'executing' }), NOW)?.text).toBe(
			'Applying…',
		);
	});

	it('treats a pending action past its expiry as expired', () => {
		expect(actionOutcome(action({ expires_at: iso(-5) }), NOW)?.text).toBe(
			'Expired — nothing was changed',
		);
	});
});

describe('helpers', () => {
	it('accepts only http(s) and root-relative result urls', () => {
		expect(safeResultUrl({ url: '/ws/tasks/1' })).toBe('/ws/tasks/1');
		expect(safeResultUrl({ url: '//evil.com' })).toBeNull();
		expect(safeResultUrl(null)).toBeNull();
	});

	it('detects long bodies', () => {
		expect(isLongBody(null)).toBe(false);
		expect(isLongBody('short')).toBe(false);
		expect(isLongBody('a'.repeat(300))).toBe(true);
		expect(isLongBody('a\nb\nc\nd\ne')).toBe(true);
	});
});
