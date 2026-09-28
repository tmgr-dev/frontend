function toNumber(value: unknown): number | null {
	if (value === null || value === undefined) return null;
	const num = typeof value === 'number' ? value : Number(value);
	return Number.isFinite(num) ? num : null;
}

export function formatCount(value: unknown): string {
	const num = toNumber(value);
	if (num === null) return '—';
	return Math.round(num).toLocaleString('en-US');
}

export function formatHours(value: unknown): string {
	const num = toNumber(value);
	if (num === null) return '—';
	return num.toLocaleString('en-US', {
		minimumFractionDigits: 0,
		maximumFractionDigits: 1,
	});
}

export function hoursToDays(value: unknown): string {
	const num = toNumber(value);
	if (num === null) return '—';
	return (num / 24).toLocaleString('en-US', {
		minimumFractionDigits: 2,
		maximumFractionDigits: 2,
	});
}

export function hoursToYears(value: unknown): string {
	const num = toNumber(value);
	if (num === null) return '—';
	return (num / 24 / 365).toLocaleString('en-US', {
		minimumFractionDigits: 2,
		maximumFractionDigits: 2,
	});
}
