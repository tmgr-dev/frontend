import {
	contextLabel,
	contextLevel,
	contextPercent,
	formatTokens,
} from '../contextMeter';

describe('formatTokens', () => {
	it.each([
		[0, '0'],
		[850, '850'],
		[1000, '1k'],
		[1500, '1.5k'],
		[9950, '10k'],
		[84000, '84k'],
		[84400, '84k'],
		[999499, '999k'],
		[999500, '1M'],
		[1000000, '1M'],
		[1200000, '1.2M'],
		[12000000, '12M'],
	])('formats %i as %s', (input, expected) => {
		expect(formatTokens(input)).toBe(expected);
	});
});

describe('contextPercent', () => {
	it('rounds the share of the limit to an integer', () => {
		expect(contextPercent(84000, 200000)).toBe(42);
		expect(contextPercent(1, 3)).toBe(33);
	});
	it('returns null when the limit is unknown or zero', () => {
		expect(contextPercent(84000, null)).toBeNull();
		expect(contextPercent(84000, 0)).toBeNull();
	});
	it('clamps to the 0..100 range', () => {
		expect(contextPercent(250000, 200000)).toBe(100);
		expect(contextPercent(-5, 200000)).toBe(0);
	});
});

describe('contextLevel', () => {
	it.each([
		[0, 'normal'],
		[69, 'normal'],
		[70, 'warning'],
		[89, 'warning'],
		[90, 'danger'],
		[100, 'danger'],
	])('maps %i%% to %s', (percent, expected) => {
		expect(contextLevel(percent)).toBe(expected);
	});
});

describe('contextLabel', () => {
	it('shows percent and tokens over the limit when the limit is known', () => {
		expect(contextLabel({ context_tokens: 84000, context_limit: 200000 })).toBe(
			'42% · 84k / 200k',
		);
		expect(
			contextLabel({ context_tokens: 84000, context_limit: 1000000 }),
		).toBe('8% · 84k / 1M');
	});
	it('shows only tokens when the limit is unknown', () => {
		expect(contextLabel({ context_tokens: 84000, context_limit: null })).toBe(
			'84k tokens',
		);
	});
});
