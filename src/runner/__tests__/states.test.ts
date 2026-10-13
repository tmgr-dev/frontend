import { describeState, sanitizeState } from '../states';

const state = (kind: string, detail: unknown = null) =>
	({ kind, sinceMs: 1, detail }) as any;

describe('describeState', () => {
	it.each([
		['starting', null, 'Starting', 'info', false],
		['working', null, 'Working', 'active', false],
		['waiting', null, 'Idle', 'neutral', false],
		['blocked', null, 'Needs input', 'attention', true],
		['exited', 'done', 'Done', 'success', false],
		['exited', 'failed', 'Failed', 'danger', false],
		['exited', 'interrupted', 'Interrupted', 'warning', false],
	])('%s/%s -> %s', (kind, detail, label, tone, highlight) => {
		expect(describeState(state(kind, detail))).toEqual({
			label,
			tone,
			highlight,
		});
	});

	it('never leaks a blocked question into the label', () => {
		const described = describeState(state('blocked', 'rm -rf / ?'));
		expect(JSON.stringify(described)).not.toContain('rm -rf');
	});

	it('falls back for unknown kinds and exits without detail', () => {
		expect(describeState(state('mystery')).label).toBe('Unknown');
		expect(describeState(state('exited')).label).toBe('Done');
	});
});

describe('sanitizeState', () => {
	it('keeps only the known exit details', () => {
		expect(sanitizeState(state('exited', 'failed')).detail).toBe('failed');
		expect(sanitizeState(state('blocked', 'secret question')).detail).toBeNull();
		expect(sanitizeState(state('exited', 'weird')).detail).toBeNull();
	});
});
