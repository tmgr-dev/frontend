import {
	barRects,
	chartScale,
	chartSummary,
	clampActive,
	formatTick,
	formatValue,
	linePath,
	nearestIndex,
	plotBox,
	stepIndex,
	visibleLabelIndexes,
	xCenter,
	yOf,
} from '../pluginChart';

const box = { left: 40, top: 10, width: 400, height: 100 };

describe('chartScale', () => {
	it('gives nice ticks that cover the data', () => {
		const s = chartScale([[0, 87]], true);
		expect(s.min).toBe(0);
		expect(s.max).toBeGreaterThanOrEqual(87);
		expect(s.ticks).toEqual([0, 20, 40, 60, 80, 100]);
	});

	it('handles negatives', () => {
		const s = chartScale([[-30, 45]], true);
		expect(s.min).toBeLessThanOrEqual(-30);
		expect(s.max).toBeGreaterThanOrEqual(45);
		expect(s.ticks).toContain(0);
	});

	it('handles all zero, no data and one point', () => {
		expect(chartScale([[0, 0]], true).ticks).toEqual([
			0, 0.2, 0.4, 0.6, 0.8, 1,
		]);
		expect(chartScale([[null]], true).max).toBeGreaterThan(0);
		const one = chartScale([[50]], false);
		expect(one.min).toBeLessThan(50);
		expect(one.max).toBeGreaterThan(50);
	});

	it('does not include zero for lines unless asked', () => {
		expect(chartScale([[100, 110]], false).min).toBeGreaterThan(0);
	});

	it('avoids float noise in ticks', () => {
		const s = chartScale([[0, 0.3]], true);
		expect(s.ticks).toContain(0.3);
	});
});

describe('formatTick', () => {
	it('compacts large numbers', () => {
		expect(formatTick(1500)).toBe('1,500');
		expect(formatTick(25000)).toBe('25k');
		expect(formatTick(2500000)).toBe('2.5M');
		expect(formatTick(0.25)).toBe('0.25');
		expect(formatTick(-3)).toBe('-3');
	});
});

describe('barRects', () => {
	const scale = chartScale([[0, 10]], true);

	it('lays grouped bars side by side inside a band', () => {
		const rects = barRects(
			[
				[10, 5],
				[4, null],
			],
			scale,
			box,
		);
		expect(rects).toHaveLength(3);
		const a = rects.find((r) => r.series === 0 && r.index === 0)!;
		const b = rects.find((r) => r.series === 1 && r.index === 0)!;
		expect(b.x).toBeGreaterThan(a.x);
		expect(a.x).toBeGreaterThanOrEqual(box.left);
		expect(a.height).toBeGreaterThan(b.height);
		expect(a.y + a.height).toBeCloseTo(box.top + box.height, 1);
	});

	it('draws negatives below the baseline', () => {
		const s = chartScale([[-10, 10]], true);
		const [neg, pos] = barRects([[-10, 10]], s, box);
		const zero = yOf(0, s, box);
		expect(neg.y).toBeCloseTo(zero, 1);
		expect(pos.y + pos.height).toBeCloseTo(zero, 1);
	});

	it('returns nothing without points', () => {
		expect(barRects([[]], scale, box)).toEqual([]);
	});
});

describe('linePath', () => {
	const scale = chartScale([[0, 10]], false);

	it('breaks the path at null', () => {
		const d = linePath([1, 2, null, 3, 4], scale, box);
		expect(d.match(/M/g)).toHaveLength(2);
	});

	it('marks an isolated point so it is visible', () => {
		const d = linePath([null, 5, null], scale, box);
		const [m, l] = d.split(' L');
		expect(m.slice(1)).toBe(l);
	});

	it('is empty for only gaps', () => {
		expect(linePath([null, null], scale, box)).toBe('');
	});
});

describe('visibleLabelIndexes', () => {
	it('shows all when they fit', () => {
		expect(visibleLabelIndexes(['a', 'b', 'c'], box)).toEqual([0, 1, 2]);
	});

	it('thins long label sets so they do not overlap', () => {
		const labels = Array.from({ length: 365 }, (_, i) => `2026-01-${i}`);
		const idx = visibleLabelIndexes(labels, box);
		expect(idx[0]).toBe(0);
		const gap = idx[1] - idx[0];
		expect((box.width / 365) * gap).toBeGreaterThanOrEqual(60);
		expect(idx.length).toBeLessThan(20);
	});

	it('handles empty', () => {
		expect(visibleLabelIndexes([], box)).toEqual([]);
	});
});

describe('nearestIndex', () => {
	it('maps a pointer to the band under it', () => {
		const idx = (x: number) => nearestIndex(x, 0, 500, 500, box, 4);
		expect(idx(41)).toBe(0);
		expect(idx(130)).toBe(0);
		expect(idx(150)).toBe(1);
		expect(idx(439)).toBe(3);
	});

	it('scales client coordinates to the view box', () => {
		expect(nearestIndex(220, 100, 250, 500, box, 4)).toBe(2);
	});

	it('is null outside the plot or without data', () => {
		expect(nearestIndex(10, 0, 500, 500, box, 4)).toBeNull();
		expect(nearestIndex(480, 0, 500, 500, box, 4)).toBeNull();
		expect(nearestIndex(100, 0, 500, 500, box, 0)).toBeNull();
		expect(nearestIndex(100, 0, 0, 500, box, 4)).toBeNull();
	});
});

describe('stepIndex', () => {
	it('starts at an end and clamps', () => {
		expect(stepIndex(null, 1, 5)).toBe(0);
		expect(stepIndex(null, -1, 5)).toBe(4);
		expect(stepIndex(4, 1, 5)).toBe(4);
		expect(stepIndex(0, -1, 5)).toBe(0);
		expect(stepIndex(2, 1, 5)).toBe(3);
		expect(stepIndex(null, 1, 0)).toBeNull();
	});
});

describe('plotBox and summary', () => {
	it('leaves room for wide tick labels', () => {
		const narrow = plotBox(600, 200, chartScale([[0, 5]], true));
		const wide = plotBox(600, 200, chartScale([[0, 5000000]], true));
		expect(wide.left).toBeGreaterThan(narrow.left);
		expect(wide.left + wide.width).toBeLessThan(600);
	});

	it('centers points in their band', () => {
		expect(xCenter(0, 4, box)).toBe(box.left + 50);
	});

	it('uses the label or builds a summary', () => {
		expect(chartSummary('bar', ['A'], 3, 'Spend')).toBe(
			'Spend Use arrow keys to read values.',
		);
		expect(chartSummary('line', ['A', 'B'], 1)).toBe(
			'Line chart of A, B over 1 point Use arrow keys to read values.',
		);
	});
});

describe('clampActive', () => {
	it('resets an index that is out of range', () => {
		expect(clampActive(null, 3)).toBeNull();
		expect(clampActive(2, 3)).toBe(2);
		expect(clampActive(3, 3)).toBeNull();
		expect(clampActive(5, 0)).toBeNull();
	});
});

describe('formatValue with missing data', () => {
	it('does not throw on undefined', () => {
		expect(formatValue(undefined)).toBe('no data');
		expect(formatValue(null)).toBe('no data');
	});
});

describe('extreme values', () => {
	const cases: [string, (number | null)[][], boolean][] = [
		['tiny range', [[1e-120, 2e-120]], true],
		['denormal bar', [[5e-324]], true],
		['denormal line', [[5e-324]], false],
		['opposite extremes', [[1e308, -1e308]], false],
		['max value line', [[Number.MAX_VALUE]], false],
		['max value bar', [[Number.MAX_VALUE]], true],
		['mixed', [[1e15, -1e15, 1e-9]], false],
	];
	it.each(cases)('stays finite for %s', (_name, rows, includeZero) => {
		const scale = chartScale(rows, includeZero);
		expect(scale.ticks.length).toBeGreaterThan(0);
		expect(scale.ticks.every(Number.isFinite)).toBe(true);
		expect(scale.max).toBeGreaterThan(scale.min);
		const b = plotBox(600, 200, scale);
		const paths = rows.map((r) => linePath(r, scale, b)).join(' ');
		expect(paths).not.toMatch(/NaN|Infinity/);
		const bars = barRects(rows, scale, b);
		expect(
			bars.every((r) => [r.x, r.y, r.width, r.height].every(Number.isFinite)),
		).toBe(true);
		expect(() => scale.ticks.map(formatTick)).not.toThrow();
	});
});
