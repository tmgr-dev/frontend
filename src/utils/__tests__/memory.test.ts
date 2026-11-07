import {
	formatCountdown,
	looksLikeTaskKey,
	newestFirst,
	secondsLeft,
} from '../memory';

const now = Date.parse('2026-11-06T21:00:00Z');

describe('secondsLeft', () => {
	it('counts down to the server timestamp', () => {
		expect(secondsLeft('2026-11-06T21:04:12Z', now)).toBe(252);
	});
	it('rounds partial seconds up and never goes negative', () => {
		expect(secondsLeft('2026-11-06T21:00:00.400Z', now)).toBe(1);
		expect(secondsLeft('2026-11-06T20:59:00Z', now)).toBe(0);
	});
	it('is null without a timer or with garbage', () => {
		expect(secondsLeft(null, now)).toBeNull();
		expect(secondsLeft('nope', now)).toBeNull();
	});
});

describe('formatCountdown', () => {
	it('formats m:ss', () => {
		expect(formatCountdown(252)).toBe('4:12');
		expect(formatCountdown(59)).toBe('0:59');
		expect(formatCountdown(0)).toBe('0:00');
		expect(formatCountdown(300)).toBe('5:00');
	});
});

describe('newestFirst', () => {
	it('reverses page order without mutating', () => {
		const entries = [{ id: 'a' }, { id: 'b' }] as any;
		expect(newestFirst(entries).map((e) => e.id)).toEqual(['b', 'a']);
		expect(entries[0].id).toBe('a');
	});
});

describe('looksLikeTaskKey', () => {
	it('recognises task keys only', () => {
		expect(looksLikeTaskKey('TM-12')).toBe(true);
		expect(looksLikeTaskKey('manual')).toBe(false);
		expect(looksLikeTaskKey(null)).toBe(false);
	});
});
