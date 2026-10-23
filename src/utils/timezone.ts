export interface WallNow {
	date: string;
	hours: number;
	minutes: number;
}

export interface TimeZoneOption {
	value: string;
	label: string;
}

const FALLBACK_ZONES = [
	'UTC',
	'Europe/London',
	'Europe/Berlin',
	'Europe/Warsaw',
	'Europe/Kyiv',
	'Europe/Moscow',
	'Asia/Dubai',
	'Asia/Kolkata',
	'Asia/Singapore',
	'Asia/Tokyo',
	'Australia/Sydney',
	'America/New_York',
	'America/Chicago',
	'America/Denver',
	'America/Los_Angeles',
	'America/Sao_Paulo',
];

export const isValidTimeZone = (tz: unknown): tz is string => {
	if (typeof tz !== 'string' || tz.trim() === '') return false;
	try {
		new Intl.DateTimeFormat('en-US', { timeZone: tz });
		return true;
	} catch {
		return false;
	}
};

export const browserTimeZone = (): string | null => {
	try {
		const tz = new Intl.DateTimeFormat().resolvedOptions().timeZone;
		if (!tz || tz === 'Etc/Unknown' || !isValidTimeZone(tz)) return null;
		return tz;
	} catch {
		return null;
	}
};

let timeZoneSource: (() => string) | null = null;

export const setTimeZoneSource = (source: (() => string) | null) => {
	timeZoneSource = source;
};

export const currentTimeZone = (): string =>
	timeZoneSource?.() || browserTimeZone() || 'UTC';

export const wallPartsIn = (tz: string, at: Date) => {
	const parts = new Intl.DateTimeFormat('en-US', {
		timeZone: tz,
		hourCycle: 'h23',
		year: 'numeric',
		month: '2-digit',
		day: '2-digit',
		hour: '2-digit',
		minute: '2-digit',
		second: '2-digit',
	}).formatToParts(at);
	const get = (type: string) =>
		Number(parts.find((p) => p.type === type)?.value);
	return {
		year: get('year'),
		month: get('month'),
		day: get('day'),
		hours: get('hour') % 24,
		minutes: get('minute'),
		seconds: get('second'),
	};
};

const pad2 = (n: number) => String(n).padStart(2, '0');

export const wallNowIn = (tz: string, at: Date = new Date()): WallNow => {
	const p = wallPartsIn(tz, at);
	return {
		date: `${p.year}-${pad2(p.month)}-${pad2(p.day)}`,
		hours: p.hours,
		minutes: p.minutes,
	};
};

export const todayIn = (tz: string, at: Date = new Date()): string =>
	wallNowIn(tz, at).date;

export const todayDateIn = (tz: string, at: Date = new Date()): Date => {
	const p = wallPartsIn(tz, at);
	return new Date(p.year, p.month - 1, p.day);
};

export const nowDateIn = (tz: string, at: Date = new Date()): Date => {
	const p = wallPartsIn(tz, at);
	return new Date(p.year, p.month - 1, p.day, p.hours, p.minutes);
};

const offsetLabel = (tz: string, at: Date): string => {
	const p = wallPartsIn(tz, at);
	const asUtc = Date.UTC(p.year, p.month - 1, p.day, p.hours, p.minutes);
	const minutes = Math.round(
		(asUtc - Math.floor(at.getTime() / 60000) * 60000) / 60000,
	);
	const sign = minutes < 0 ? '-' : '+';
	const abs = Math.abs(minutes);
	return `UTC${sign}${pad2(Math.floor(abs / 60))}:${pad2(abs % 60)}`;
};

const supportedZones = (): string[] => {
	try {
		const list = (Intl as any).supportedValuesOf?.('timeZone');
		if (Array.isArray(list) && list.length) return list;
	} catch {}
	return FALLBACK_ZONES;
};

export const timeZoneOptions = (
	current?: string | null,
	at: Date = new Date(),
): TimeZoneOption[] => {
	const zones = new Set(supportedZones());
	zones.add('UTC');
	const detected = browserTimeZone();
	if (detected) zones.add(detected);
	if (current) zones.add(current);
	return [...zones]
		.filter(isValidTimeZone)
		.sort()
		.map((value) => ({ value, label: `${value} (${offsetLabel(value, at)})` }));
};
