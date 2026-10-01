import type { PageType } from '@/actions/tmgr/pages';
import { NETWORK_OPTIONS } from '@/utils/pages/properties';

export interface PropertyDef {
	key: string;
	label: string;
	kind:
		| 'string'
		| 'date'
		| 'enum'
		| 'readonlyDate'
		| 'user'
		| 'aliases'
		| 'participants'
		| 'tasks';
	options?: { value: string; label: string }[];
}

export const PROPERTY_DEFS: Partial<Record<PageType, PropertyDef[]>> = {
	person: [
		{ key: 'user_id', label: 'Участник', kind: 'user' },
		{ key: 'network', label: 'Связь', kind: 'enum', options: NETWORK_OPTIONS },
		{ key: 'company', label: 'Компания', kind: 'string' },
		{ key: 'role', label: 'Роль', kind: 'string' },
		{ key: 'aliases', label: 'Псевдонимы', kind: 'aliases' },
		{
			key: 'last_contact_at',
			label: 'Последний контакт',
			kind: 'readonlyDate',
		},
	],
	meeting: [
		{ key: 'date', label: 'Дата', kind: 'date' },
		{ key: 'participants', label: 'Участники', kind: 'participants' },
		{ key: 'related_tasks', label: 'Задачи', kind: 'tasks' },
	],
};

export const TYPE_LABELS: Record<PageType, string> = {
	plain: 'Страница',
	context: 'Контекст',
	person: 'Человек',
	meeting: 'Встреча',
};

export const propertyDefsFor = (type: PageType): PropertyDef[] =>
	PROPERTY_DEFS[type] ?? [];
