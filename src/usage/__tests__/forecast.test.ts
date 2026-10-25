import { forecastLimit, forecastText } from '../forecast';

const HOUR = 3_600_000;
const NOW = Date.parse('2026-10-25T12:00:00Z');
const iso = (ms: number) => new Date(ms).toISOString();

const limit = (over: Record<string, unknown> = {}) => ({
	used_pct: 30,
	window_s: 18000,
	resets_at: iso(NOW + 2 * HOUR),
	observed_at: iso(NOW),
	...over,
});

const check = (over: Record<string, unknown> = {}, now = NOW) =>
	forecastLimit(limit(over) as Parameters<typeof forecastLimit>[0], now);

describe('forecastLimit', () => {
	it('says when the limit runs out before the reset', () => {
		const f = check({ used_pct: 80 });
		expect(f).toEqual({ kind: 'runs_out', seconds: 2700 });
		expect(forecastText(f)).toBe('Runs out in 45m at current pace');
	});

	it('counts the time to exhaustion from now, not from the reading', () => {
		const f = check({ used_pct: 80, observed_at: iso(NOW - 10 * 60_000) });
		expect(f).toEqual({ kind: 'runs_out', seconds: 1950 });
	});

	it('never shows less than a minute', () => {
		expect(forecastText({ kind: 'runs_out', seconds: 1 })).toBe(
			'Runs out in <1m at current pace',
		);
	});

	it('says the limit may be reached when the projected run-out already passed', () => {
		const f = check({ used_pct: 99, observed_at: iso(NOW - 20 * 60_000) });
		expect(f).toEqual({ kind: 'maybe_reached' });
		expect(forecastText(f)).toBe('May already be at the limit at current pace');
	});

	it('lasts until the reset when the pace is slow', () => {
		const f = check({ used_pct: 30 });
		expect(f).toEqual({ kind: 'lasts' });
		expect(forecastText(f)).toBe('Lasts until the reset at current pace');
	});

	it('treats exhaustion exactly at the reset as lasting', () => {
		expect(check({ used_pct: 80, resets_at: iso(NOW + HOUR) })).toEqual({
			kind: 'lasts',
		});
	});

	it('reports a reached limit, even without a reset time', () => {
		expect(check({ used_pct: 100 })).toEqual({ kind: 'reached' });
		expect(check({ used_pct: 120, resets_at: null, window_s: null })).toEqual({
			kind: 'reached',
		});
		expect(forecastText({ kind: 'reached' })).toBe('Limit reached');
	});

	it('has no forecast when the reset already passed', () => {
		expect(check({ used_pct: 100, resets_at: iso(NOW - 1000) })).toEqual({
			kind: 'none',
		});
		expect(check({ used_pct: 50, resets_at: iso(NOW) })).toEqual({
			kind: 'none',
		});
	});

	it('has no forecast without a reset time', () => {
		expect(check({ resets_at: null })).toEqual({ kind: 'none' });
		expect(check({ resets_at: 'soon' })).toEqual({ kind: 'none' });
	});

	it('has no forecast without a window length', () => {
		expect(check({ window_s: null })).toEqual({ kind: 'none' });
		expect(check({ window_s: 0 })).toEqual({ kind: 'none' });
	});

	it('has no forecast below 1% used', () => {
		expect(check({ used_pct: 0.5 })).toEqual({ kind: 'none' });
		expect(check({ used_pct: 1 }).kind).toBe('lasts');
	});

	it('has no forecast early in the window (10% of the window)', () => {
		const early = { resets_at: iso(NOW + 4.8 * HOUR) };
		expect(check({ ...early, used_pct: 5 })).toEqual({ kind: 'none' });
		const enough = { resets_at: iso(NOW + 4.4 * HOUR) };
		expect(check({ ...enough, used_pct: 5 }).kind).toBe('lasts');
	});

	it('has no forecast in the first 15 minutes of a short window', () => {
		const short = { window_s: 1800, used_pct: 90 };
		expect(check({ ...short, resets_at: iso(NOW + 20 * 60_000) })).toEqual({
			kind: 'none',
		});
		expect(check({ ...short, resets_at: iso(NOW + 10 * 60_000) }).kind).toBe(
			'runs_out',
		);
	});

	it('has no forecast from a reading older than 25% of the window', () => {
		const resets = iso(NOW + 2 * HOUR);
		const stale = iso(NOW - 1.5 * HOUR);
		expect(check({ observed_at: stale, resets_at: resets })).toEqual({
			kind: 'none',
		});
		expect(check({ observed_at: iso(NOW - 1 * HOUR) }).kind).not.toBe('none');
	});

	it('has no forecast for an unreadable observation time', () => {
		expect(check({ observed_at: 'n/a' })).toEqual({ kind: 'none' });
		expect(forecastText({ kind: 'none' })).toBe(
			'Not enough data for a forecast yet',
		);
	});
});
