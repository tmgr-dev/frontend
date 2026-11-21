import {
	ageLabel,
	drainStyle,
	glyphOf,
	heightOf,
	hoverKey,
	isOk,
	progressFraction,
} from '../model';

describe('toast model', () => {
	it('labels the age of a toast', () => {
		expect(ageLabel(10_000, 10_000)).toBe('now');
		expect(ageLabel(10_000, 20_000)).toBe('now');
		expect(ageLabel(130_000, 10_000)).toBe('2m');
		expect(ageLabel(3 * 3_600_000 + 5_000, 0)).toBe('3h');
	});

	it('picks a glyph and the ok tint per kind', () => {
		expect(glyphOf('decision')).toBe('?');
		expect(glyphOf('limit')).toBe('%');
		expect(isOk('agent')).toBe(true);
		expect(isOk('idle')).toBe(false);
	});

	it('rounds the measured height up', () => {
		expect(heightOf(120.2)).toBe(121);
		expect(heightOf(-3)).toBe(0);
	});

	it('computes the countdown fraction', () => {
		expect(progressFraction(5_000, 10_000)).toBe(0.5);
		expect(progressFraction(20_000, 10_000)).toBe(1);
		expect(progressFraction(-5, 10_000)).toBe(0);
		expect(progressFraction(null, 10_000)).toBeNull();
		expect(progressFraction(5_000, null)).toBeNull();
		expect(progressFraction(5_000, 0)).toBeNull();
	});

	it('builds the drain style from the remaining time', () => {
		expect(
			drainStyle({ remainingMs: 2_500, totalMs: 10_000, paused: false }),
		).toEqual({ '--from': '0.25', '--drain': '2500ms', '--state': 'running' });
		expect(
			drainStyle({ remainingMs: 2_500, totalMs: 10_000, paused: true }),
		)?.toMatchObject({ '--state': 'paused' });
		expect(drainStyle({ remainingMs: null, totalMs: null })).toBeNull();
	});

	it('pauses while hovered or focused', () => {
		expect(hoverKey(false, false)).toBe(false);
		expect(hoverKey(true, false)).toBe(true);
		expect(hoverKey(false, true)).toBe(true);
	});
});
