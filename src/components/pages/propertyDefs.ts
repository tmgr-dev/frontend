import type { PageType } from '@/actions/tmgr/pages';
import { NETWORK_OPTIONS } from '@/utils/pages/properties';

export interface PropertyDef {
	key: string;
	labelKey: string;
	kind:
		| 'string'
		| 'date'
		| 'enum'
		| 'readonlyDate'
		| 'user'
		| 'aliases'
		| 'participants'
		| 'tasks';
	options?: { value: string; labelKey: string }[];
}

export const PROPERTY_DEFS: Partial<Record<PageType, PropertyDef[]>> = {
	person: [
		{ key: 'user_id', labelKey: 'pages.helpers.property.member', kind: 'user' },
		{
			key: 'network',
			labelKey: 'pages.helpers.property.relationship',
			kind: 'enum',
			options: NETWORK_OPTIONS,
		},
		{
			key: 'company',
			labelKey: 'pages.helpers.property.company',
			kind: 'string',
		},
		{ key: 'role', labelKey: 'pages.helpers.property.role', kind: 'string' },
		{
			key: 'aliases',
			labelKey: 'pages.helpers.property.aliases',
			kind: 'aliases',
		},
		{
			key: 'last_contact_at',
			labelKey: 'pages.helpers.property.lastContact',
			kind: 'readonlyDate',
		},
	],
	meeting: [
		{ key: 'date', labelKey: 'pages.helpers.property.date', kind: 'date' },
		{
			key: 'participants',
			labelKey: 'pages.helpers.property.participants',
			kind: 'participants',
		},
		{
			key: 'related_tasks',
			labelKey: 'pages.helpers.property.tasks',
			kind: 'tasks',
		},
	],
};

export const propertyDefsFor = (type: PageType): PropertyDef[] =>
	PROPERTY_DEFS[type] ?? [];
