import type { PageType } from '@/actions/tmgr/pages';

export interface PropertyDef {
	key: string;
	label: string;
	kind: 'string' | 'date' | 'enum';
	options?: { value: string; label: string }[];
}

export const PROPERTY_DEFS: Partial<Record<PageType, PropertyDef[]>> = {};

export const TYPE_LABELS: Record<PageType, string> = {
	plain: 'Страница',
	context: 'Контекст',
	person: 'Человек',
	meeting: 'Встреча',
};

export const propertyDefsFor = (type: PageType): PropertyDef[] =>
	PROPERTY_DEFS[type] ?? [];
