export const ONE_DAY = 86_400_000;

export function startOfDay(d: Date): Date {
	const x = new Date(d);
	x.setHours(0, 0, 0, 0);
	return x;
}

export function isSameDay(a: Date, b: Date): boolean {
	return startOfDay(a).getTime() === startOfDay(b).getTime();
}

export function addDays(d: Date, n: number): Date {
	const x = new Date(d);
	x.setDate(x.getDate() + n);
	return x;
}

export function startOfWeek(d: Date): Date {
	// Monday-start
	const x = startOfDay(d);
	const dow = (x.getDay() + 6) % 7;
	x.setDate(x.getDate() - dow);
	return x;
}

export function startOfMonth(d: Date): Date {
	const x = startOfDay(d);
	x.setDate(1);
	return x;
}

export function fmtTime(h: number, m = 0): string {
	return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}`;
}

export function fmtDate(d: Date): string {
	const y = d.getFullYear();
	const m = String(d.getMonth() + 1).padStart(2, '0');
	const day = String(d.getDate()).padStart(2, '0');
	return `${y}-${m}-${day}`;
}

export function dowShort(d: Date): string {
	return ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'][(d.getDay() + 6) % 7];
}

export function monthShort(d: Date): string {
	return ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'][d.getMonth()];
}

export function parseTime(t: string | null | undefined): { h: number; m: number } | null {
	if (!t) return null;
	const parts = t.split(':');
	return { h: parseInt(parts[0], 10) || 0, m: parseInt(parts[1], 10) || 0 };
}
