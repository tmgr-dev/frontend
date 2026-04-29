import type { RoutineEntry } from '@/types/dailyRoutine';
import { monthShort } from './dateHelpers';

export interface RecurrenceLike {
	frequency: string;
	interval?: number | null;
	days_of_week?: string[] | null;
	day_of_frequency?: number | null;
	month?: number | null;
}

const DOW_SHORT = ['Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa', 'Su'];
const DOW_KEYS = ['MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT', 'SUN'];

function dowKeyToIndex(key: string): number {
	return DOW_KEYS.indexOf(key.toUpperCase());
}

export function recurrenceLabel(r: RecurrenceLike): string {
	if (r.frequency === 'DAILY') return (r.interval ?? 1) === 1 ? 'Daily' : `Every ${r.interval} days`;
	if (r.frequency === 'WEEKLY') {
		const days = (r.days_of_week ?? []).map(d => dowKeyToIndex(d)).filter(i => i >= 0).sort();
		if (days.length === 0) return 'Weekly';
		if (days.length === 7) return 'Daily';
		if (days.join(',') === '0,1,2,3,4') return 'Weekdays';
		if (days.join(',') === '5,6') return 'Weekends';
		return days.map(d => DOW_SHORT[d]).join(' · ');
	}
	if (r.frequency === 'MONTHLY') return `Monthly · day ${r.day_of_frequency ?? 1}`;
	if (r.frequency === 'YEARLY') {
		const monthIdx = r.month ?? 0;
		const dom = r.day_of_frequency ?? 1;
		return `Yearly · ${monthShort(new Date(2026, monthIdx, dom))} ${dom}`;
	}
	return '';
}

export function entryRecurrenceLabel(entry: RoutineEntry): string {
	return recurrenceLabel({ frequency: entry.frequency });
}
