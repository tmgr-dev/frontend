import {
	SCOPE_DESCRIPTIONS,
	describeScope,
	isCompleteUserCode,
	normalizeUserCode,
	sameScopes,
	toggleScope,
	visibleScopes,
} from '../deviceLogin';

describe('normalizeUserCode', () => {
	it('uppercases and inserts the dash', () => {
		expect(normalizeUserCode('abcd2345')).toBe('ABCD-2345');
		expect(normalizeUserCode('abcd-2345')).toBe('ABCD-2345');
		expect(normalizeUserCode('  ab cd 23 45 ')).toBe('ABCD-2345');
	});

	it('handles partial, empty and missing input', () => {
		expect(normalizeUserCode('abc')).toBe('ABC');
		expect(normalizeUserCode('abcd')).toBe('ABCD');
		expect(normalizeUserCode('abcde')).toBe('ABCD-E');
		expect(normalizeUserCode('')).toBe('');
		expect(normalizeUserCode(undefined)).toBe('');
		expect(normalizeUserCode(null)).toBe('');
	});

	it('drops extra characters beyond eight', () => {
		expect(normalizeUserCode('abcd23456789')).toBe('ABCD-2345');
	});
});

describe('isCompleteUserCode', () => {
	it('accepts only full XXXX-XXXX codes', () => {
		expect(isCompleteUserCode('ABCD-2345')).toBe(true);
		expect(isCompleteUserCode('ABCD-234')).toBe(false);
		expect(isCompleteUserCode('ABCD2345')).toBe(false);
		expect(isCompleteUserCode('')).toBe(false);
	});
});

describe('scope descriptions', () => {
	it('describes all twelve scopes', () => {
		expect(Object.keys(SCOPE_DESCRIPTIONS)).toHaveLength(12);
		expect(describeScope('tasks:create')).toBe('Create tasks');
		expect(describeScope('notify:send')).toBe(
			'Send notifications and phone alarms to your devices',
		);
	});

	it('falls back to the raw scope name', () => {
		expect(describeScope('future:scope')).toBe('future:scope');
	});
});

describe('scope helpers', () => {
	it('toggles without mutating', () => {
		const start = ['a', 'b'];
		expect(toggleScope(start, 'a')).toEqual(['b']);
		expect(toggleScope(start, 'c')).toEqual(['a', 'b', 'c']);
		expect(start).toEqual(['a', 'b']);
	});

	it('compares sets regardless of order', () => {
		expect(sameScopes(['a', 'b'], ['b', 'a'])).toBe(true);
		expect(sameScopes(['a'], ['a', 'b'])).toBe(false);
	});

	it('hides the internal cli marker', () => {
		expect(visibleScopes(['cli', 'tasks:read'])).toEqual(['tasks:read']);
		expect(visibleScopes(undefined)).toEqual([]);
	});
});
