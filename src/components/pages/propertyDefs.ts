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
		{ key: 'user_id', label: 'Member', kind: 'user' },
		{ key: 'network', label: 'Relationship', kind: 'enum', options: NETWORK_OPTIONS },
		{ key: 'company', label: 'Company', kind: 'string' },
		{ key: 'role', label: 'Role', kind: 'string' },
		{ key: 'aliases', label: 'Aliases', kind: 'aliases' },
		{
			key: 'last_contact_at',
			label: 'Last contact',
			kind: 'readonlyDate',
		},
	],
	meeting: [
		{ key: 'date', label: 'Date', kind: 'date' },
		{ key: 'participants', label: 'Participants', kind: 'participants' },
		{ key: 'related_tasks', label: 'Tasks', kind: 'tasks' },
	],
};

export const TYPE_LABELS: Record<PageType, string> = {
	plain: 'Plain page',
	context: 'Context',
	person: 'Person',
	meeting: 'Meeting',
};

export const propertyDefsFor = (type: PageType): PropertyDef[] =>
	PROPERTY_DEFS[type] ?? [];
