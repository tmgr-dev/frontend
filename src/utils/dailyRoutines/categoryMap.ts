import type { RoutineCategory, RoutineCategoryId } from '@/types/dailyRoutine';

export const ROUTINE_CATEGORIES: Record<RoutineCategoryId, RoutineCategory> = {
	work:   { id: 'work',   name: 'Work',    color: '#5b8cff' },
	health: { id: 'health', name: 'Health',  color: '#22c55e' },
	learn:  { id: 'learn',  name: 'Learn',   color: '#a78bfa' },
	home:   { id: 'home',   name: 'Home',    color: '#f5b54a' },
	social: { id: 'social', name: 'Social',  color: '#ec4899' },
	none:   { id: 'none',   name: 'General', color: '#888888' },
};

export const ROUTINE_CATEGORY_LIST: RoutineCategory[] = Object.values(ROUTINE_CATEGORIES);

export function resolveCategory(id?: string | null): RoutineCategory {
	if (!id) return ROUTINE_CATEGORIES.none;
	return (ROUTINE_CATEGORIES as Record<string, RoutineCategory>)[id] ?? ROUTINE_CATEGORIES.none;
}

export function hexAlpha(hex: string, a: number): string {
	const h = hex.replace('#', '');
	const r = parseInt(h.slice(0, 2), 16);
	const g = parseInt(h.slice(2, 4), 16);
	const b = parseInt(h.slice(4, 6), 16);
	return `rgba(${r},${g},${b},${a})`;
}
