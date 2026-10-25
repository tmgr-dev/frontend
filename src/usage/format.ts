export const money = (usd: number | null | undefined): string => {
	const value = usd == null ? NaN : Number(usd);
	if (!Number.isFinite(value)) return '—';
	if (value > 0 && value < 0.01) return '<$0.01';
	if (value >= 1000) return `$${Math.round(value).toLocaleString('en-US')}`;
	return `$${value.toFixed(2)}`;
};

export const count = (n: number | null | undefined): string => {
	const value = n == null ? NaN : Number(n);
	if (!Number.isFinite(value)) return '—';
	const abs = Math.abs(value);
	const units: [number, string][] = [
		[1e9, 'B'],
		[1e6, 'M'],
		[1e3, 'K'],
	];
	for (const [size, suffix] of units) {
		if (abs >= size) {
			const scaled = value / size;
			const text =
				Math.abs(scaled) >= 100 ? scaled.toFixed(0) : scaled.toFixed(1);
			return `${text.replace(/\.0$/, '')}${suffix}`;
		}
	}
	return String(Math.round(value));
};

export const span = (seconds: number | null | undefined): string => {
	if (seconds === null || seconds === undefined || !Number.isFinite(seconds))
		return '—';
	if (seconds <= 0) return 'now';
	const s = Math.floor(seconds);
	const days = Math.floor(s / 86400);
	const hours = Math.floor((s % 86400) / 3600);
	const minutes = Math.floor((s % 3600) / 60);
	if (days > 0) return `${days}d ${hours}h`;
	if (hours > 0) return `${hours}h ${minutes}m`;
	if (minutes > 0) return `${minutes}m`;
	return '<1m';
};

export const ago = (seconds: number | null | undefined): string => {
	if (seconds === null || seconds === undefined || !Number.isFinite(seconds))
		return '—';
	return seconds < 60 ? 'just now' : `${span(seconds)} ago`;
};

const TOOL_NAMES: Record<string, string> = {
	claude: 'Claude Code',
	codex: 'Codex',
};

export const toolName = (tool: string): string =>
	TOOL_NAMES[tool] ?? (tool ? tool[0].toUpperCase() + tool.slice(1) : '—');

export const limitLabel = (limit: {
	tool: string;
	window_label: string;
}): string => `${toolName(limit.tool)} · ${limit.window_label}`;

export const percent = (value: number | null | undefined): number => {
	const n = Number(value);
	if (!Number.isFinite(n)) return 0;
	return Math.min(100, Math.max(0, Math.round(n)));
};

export type BarTone = 'ok' | 'warn' | 'danger';

export const barTone = (value: number | null | undefined): BarTone => {
	const p = percent(value);
	if (p >= 90) return 'danger';
	if (p >= 70) return 'warn';
	return 'ok';
};

export const axisMoney = (usd: number): string =>
	`$${Number.isInteger(usd) ? usd.toLocaleString('en-US') : usd.toFixed(2)}`;

export const share = (value: number): string => {
	if (!Number.isFinite(value)) return '—';
	if (value > 0 && value < 1) return '<1%';
	return `${Math.round(value)}%`;
};

export const signedPercent = (value: number): string => {
	const rounded = Math.round(value);
	return `${rounded > 0 ? '+' : rounded < 0 ? '−' : ''}${Math.abs(rounded)}%`;
};

const MONTHS = [
	'Jan',
	'Feb',
	'Mar',
	'Apr',
	'May',
	'Jun',
	'Jul',
	'Aug',
	'Sep',
	'Oct',
	'Nov',
	'Dec',
];
const WEEKDAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

const dayParts = (day: string): [number, number, number] => {
	const [y, m, d] = day.split('-').map(Number);
	return [y, m - 1, d];
};

export const shortDay = (day: string): string => {
	const [, m, d] = dayParts(day);
	return `${MONTHS[m] ?? '—'} ${d}`;
};

export const longDay = (day: string): string => {
	const [y, m, d] = dayParts(day);
	return `${WEEKDAYS[new Date(y, m, d).getDay()]}, ${shortDay(day)}`;
};
