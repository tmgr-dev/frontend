import type { Color } from '@/pluginSystem/uiTree';

export interface ChartBox {
	left: number;
	top: number;
	width: number;
	height: number;
}

export interface ChartScale {
	min: number;
	max: number;
	ticks: number[];
}

export interface ChartBar {
	series: number;
	index: number;
	x: number;
	y: number;
	width: number;
	height: number;
}

export const CHART_DEFAULT_WIDTH = 600;
const PAD_RIGHT = 10;
const PAD_TOP = 10;
const PAD_BOTTOM = 26;
const TICK_CHAR_WIDTH = 6.5;
const LABEL_CHAR_WIDTH = 6;
const LABEL_GAP = 10;
export const MAX_AXIS_LABEL = 16;

export const CHART_STROKE_CLASS: Record<Color, string> = {
	gray: 'text-gray-500 dark:text-gray-400',
	green: 'text-emerald-500 dark:text-emerald-400',
	yellow: 'text-amber-500 dark:text-amber-400',
	red: 'text-red-500 dark:text-red-400',
	blue: 'text-sky-500 dark:text-sky-400',
	purple: 'text-purple-500 dark:text-purple-400',
	orange: 'text-orange-500 dark:text-orange-400',
};

const round = (n: number) => Math.round(n * 100) / 100;

export const niceStep = (range: number, target: number): number => {
	const rough = range / Math.max(1, target);
	const magnitude = Math.pow(10, Math.floor(Math.log10(rough)));
	const residual = rough / magnitude;
	const factor = residual <= 1 ? 1 : residual <= 2 ? 2 : residual <= 5 ? 5 : 10;
	return factor * magnitude;
};

const FALLBACK_SCALE: ChartScale = { min: 0, max: 1, ticks: [0, 1] };
const MAX_DECIMALS = 20;

export const chartScale = (
	values: (number | null)[][],
	includeZero: boolean,
	target = 5,
): ChartScale => {
	let lo = Infinity;
	let hi = -Infinity;
	for (const row of values) {
		for (const v of row) {
			if (v === null) continue;
			if (v < lo) lo = v;
			if (v > hi) hi = v;
		}
	}
	if (lo === Infinity) {
		lo = 0;
		hi = 0;
	}
	if (includeZero) {
		lo = Math.min(lo, 0);
		hi = Math.max(hi, 0);
	}
	if (lo === hi) {
		if (lo === 0) {
			hi = 1;
		} else {
			const pad = Math.abs(lo) * 0.1;
			lo -= pad;
			hi += pad;
		}
	}
	const range = hi - lo;
	const step = niceStep(range, target);
	if (!Number.isFinite(range) || !Number.isFinite(step) || step <= 0) {
		return { ...FALLBACK_SCALE, ticks: [...FALLBACK_SCALE.ticks] };
	}
	const decimals = Math.max(0, -Math.floor(Math.log10(step)));
	if (decimals > MAX_DECIMALS) {
		return { ...FALLBACK_SCALE, ticks: [...FALLBACK_SCALE.ticks] };
	}
	const first = Math.floor(lo / step + 1e-9);
	const last = Math.ceil(hi / step - 1e-9);
	const ticks: number[] = [];
	for (let k = first; k <= last; k++) {
		ticks.push(Number((k * step).toFixed(decimals)));
	}
	const min = ticks[0];
	const max = ticks[ticks.length - 1];
	if (!ticks.every(Number.isFinite) || !(max > min)) {
		return { ...FALLBACK_SCALE, ticks: [...FALLBACK_SCALE.ticks] };
	}
	return { min, max, ticks };
};

export const formatTick = (value: number): string => {
	const abs = Math.abs(value);
	const compact = (n: number, suffix: string) =>
		`${Number(n.toFixed(2))}${suffix}`;
	if (abs >= 1e9) return compact(value / 1e9, 'B');
	if (abs >= 1e6) return compact(value / 1e6, 'M');
	if (abs >= 1e4) return compact(value / 1e3, 'k');
	return Number(value.toFixed(2)).toLocaleString('en-US');
};

export const formatValue = (value: number | null | undefined): string =>
	value == null
		? 'no data'
		: Number(value.toFixed(4)).toLocaleString('en-US', {
				maximumFractionDigits: 4,
			});

export const shortAxisLabel = (label: string): string =>
	label.length > MAX_AXIS_LABEL
		? `${label.slice(0, MAX_AXIS_LABEL - 1)}…`
		: label;

export const plotBox = (
	width: number,
	height: number,
	scale: ChartScale,
): ChartBox => {
	const widest = Math.max(...scale.ticks.map((t) => formatTick(t).length));
	const left = Math.round(widest * TICK_CHAR_WIDTH + 12);
	return {
		left,
		top: PAD_TOP,
		width: Math.max(1, width - left - PAD_RIGHT),
		height: Math.max(1, height - PAD_TOP - PAD_BOTTOM),
	};
};

export const yOf = (
	value: number,
	scale: ChartScale,
	box: ChartBox,
): number => {
	const clamped = Math.min(scale.max, Math.max(scale.min, value));
	return (
		box.top +
		box.height -
		((clamped - scale.min) / (scale.max - scale.min)) * box.height
	);
};

export const xCenter = (index: number, count: number, box: ChartBox): number =>
	box.left + ((index + 0.5) / count) * box.width;

export const barRects = (
	series: (number | null)[][],
	scale: ChartScale,
	box: ChartBox,
): ChartBar[] => {
	const count = series[0]?.length ?? 0;
	if (count === 0) return [];
	const band = box.width / count;
	const group = band * 0.8;
	const barWidth = group / series.length;
	const base = yOf(Math.min(Math.max(0, scale.min), scale.max), scale, box);
	const out: ChartBar[] = [];
	for (let s = 0; s < series.length; s++) {
		const row = series[s];
		for (let i = 0; i < count; i++) {
			const v = row[i];
			if (v === null || v === undefined) continue;
			const y = yOf(v, scale, box);
			out.push({
				series: s,
				index: i,
				x: round(box.left + i * band + (band - group) / 2 + s * barWidth),
				y: round(Math.min(y, base)),
				width: round(Math.max(0.5, barWidth - (series.length > 1 ? 0.5 : 0))),
				height: round(Math.abs(base - y)),
			});
		}
	}
	return out;
};

export const linePath = (
	values: (number | null)[],
	scale: ChartScale,
	box: ChartBox,
): string => {
	const count = values.length;
	let d = '';
	let open = false;
	for (let i = 0; i < count; i++) {
		const v = values[i];
		if (v === null) {
			open = false;
			continue;
		}
		const x = round(xCenter(i, count, box));
		const y = round(yOf(v, scale, box));
		const lonely = values[i + 1] === null || values[i + 1] === undefined;
		if (!open) {
			d += `${d ? ' ' : ''}M${x},${y}`;
			open = true;
			if (lonely) d += ` L${x},${y}`;
		} else {
			d += ` L${x},${y}`;
		}
	}
	return d;
};

export const visibleLabelIndexes = (
	labels: string[],
	box: ChartBox,
): number[] => {
	const count = labels.length;
	if (count === 0) return [];
	const widest = Math.max(
		...labels.map((l) => shortAxisLabel(l).length * LABEL_CHAR_WIDTH),
	);
	const band = box.width / count;
	const step = Math.max(1, Math.ceil((widest + LABEL_GAP) / band));
	const out: number[] = [];
	for (let i = 0; i < count; i += step) out.push(i);
	return out;
};

export const nearestIndex = (
	clientX: number,
	rectLeft: number,
	rectWidth: number,
	viewWidth: number,
	box: ChartBox,
	count: number,
): number | null => {
	if (count === 0 || rectWidth <= 0) return null;
	const x = ((clientX - rectLeft) / rectWidth) * viewWidth;
	if (x < box.left || x > box.left + box.width) return null;
	return Math.min(
		count - 1,
		Math.max(0, Math.floor(((x - box.left) / box.width) * count)),
	);
};

export const stepIndex = (
	current: number | null,
	delta: number,
	count: number,
): number | null => {
	if (count === 0) return null;
	if (current === null) return delta > 0 ? 0 : count - 1;
	return Math.min(count - 1, Math.max(0, current + delta));
};

export const clampActive = (
	current: number | null,
	count: number,
): number | null =>
	current !== null && (current < 0 || current >= count) ? null : current;

export const CHART_KEY_HINT = 'Use arrow keys to read values.';

export const chartSummary = (
	kind: 'bar' | 'line',
	seriesNames: string[],
	count: number,
	label?: string,
): string =>
	`${label ?? `${kind === 'bar' ? 'Bar' : 'Line'} chart of ${seriesNames.join(', ')} over ${count} ${count === 1 ? 'point' : 'points'}`} ${CHART_KEY_HINT}`;
