import { describeState, exitCodeLabel, sanitizeState } from '../states';

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
		expect(
			sanitizeState(state('blocked', 'secret question')).detail,
		).toBeNull();
		expect(sanitizeState(state('exited', 'weird')).detail).toBeNull();
	});
});

describe('exit code', () => {
	const exited = (detail: unknown, exitCode: unknown) =>
		({ kind: 'exited', sinceMs: 1, detail, exitCode }) as any;

	it('is kept and labelled only for failed exits', () => {
		expect(sanitizeState(exited('failed', 2)).exitCode).toBe(2);
		expect(exitCodeLabel(sanitizeState(exited('failed', 2)))).toBe(
			'Exited (code 2)',
		);
		expect(sanitizeState(exited('done', 0))).not.toHaveProperty('exitCode');
		expect(exitCodeLabel(exited('done', 0))).toBeNull();
		expect(exitCodeLabel(exited('interrupted', 130))).toBeNull();
	});

	it('drops non-integer codes', () => {
		expect(sanitizeState(exited('failed', 'oops'))).not.toHaveProperty(
			'exitCode',
		);
		expect(exitCodeLabel(exited('failed', null))).toBeNull();
		expect(exitCodeLabel(exited('failed', undefined))).toBeNull();
	});

	it('is not kept for non-exited states', () => {
		expect(
			sanitizeState({ kind: 'working', sinceMs: 1, detail: null, exitCode: 1 }),
		).not.toHaveProperty('exitCode');
	});
});
