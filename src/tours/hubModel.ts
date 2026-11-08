import { t } from '@/i18n';
import { tourTitle } from './i18n';
import type { TourGroup } from './types';
import type { HubItem } from './useTours';

const GROUP_ORDER: TourGroup[] = [
	'start',
	'tasks',
	'time',
	'agents',
	'personas',
	'pages',
	'graph',
	'routines',
	'workspace',
	'desktop',
];

export const groupTitle = (group: TourGroup): string =>
	t(`onboarding.tours.groups.${group}`);

export type HubAction = 'start' | 'resume' | 'done' | 'none';

export interface HubRow {
	id: string;
	title: string;
	minutes: number;
	action: HubAction;
	updated: boolean;
	progress: string | null;
	disabledMessage: string | null;
}

export interface HubGroup {
	group: TourGroup;
	title: string;
	rows: HubRow[];
}

export interface HubModel {
	groups: HubGroup[];
	done: number;
	total: number;
}

export interface HubContext {
	canManageModules: boolean;
	isModuleEnabled(key: string): boolean;
}

export const moduleLabel = (key: string): string => {
	const text = key.replace(/[._]+/g, ' ').trim();
	return text.charAt(0).toUpperCase() + text.slice(1);
};

export const moduleOffMessage = (
	moduleKey: string,
	canManageModules: boolean,
): string => {
	const label = moduleLabel(moduleKey);
	return canManageModules
		? t('onboarding.tours.hub.turnOnModule', { label })
		: t('onboarding.tours.hub.askOwnerModule', { label });
};

const rowFor = (item: HubItem, context: HubContext): HubRow | null => {
	const { tour, state } = item;
	const base = {
		id: tour.id,
		title: tourTitle(tour),
		minutes: tour.minutes,
		updated: false,
		progress: null as string | null,
		disabledMessage: null as string | null,
	};
	if (!item.available) {
		if (item.unavailableReason !== 'module_off') return null;
		const missing =
			tour.requires?.modules?.find((key) => !context.isModuleEnabled(key)) ??
			tour.requires?.modules?.[0] ??
			tour.group;
		return {
			...base,
			action: 'none',
			disabledMessage: moduleOffMessage(missing, context.canManageModules),
		};
	}
	if (state?.status === 'completed') {
		const outdated = state.version < tour.version;
		return { ...base, action: outdated ? 'start' : 'done', updated: outdated };
	}
	if (state?.status === 'in_progress' || state?.status === 'paused') {
		const index = tour.steps.findIndex((step) => step.id === state.step_id);
		const position = index >= 0 ? index + 1 : 1;
		return {
			...base,
			action: 'resume',
			progress: t('onboarding.tours.hub.progress', {
				position,
				total: tour.steps.length,
			}),
		};
	}
	return { ...base, action: 'start' };
};

export const buildHubModel = (
	items: HubItem[],
	context: HubContext,
): HubModel => {
	const groups = new Map<TourGroup, HubRow[]>();
	let done = 0;
	let total = 0;
	for (const item of items) {
		const row = rowFor(item, context);
		if (!row) continue;
		if (row.action !== 'none') {
			total += 1;
			if (row.action === 'done') done += 1;
		}
		const rows = groups.get(item.tour.group) ?? [];
		rows.push(row);
		groups.set(item.tour.group, rows);
	}
	return {
		done,
		total,
		groups: GROUP_ORDER.filter((group) => groups.has(group)).map((group) => ({
			group,
			title: groupTitle(group),
			rows: groups.get(group)!,
		})),
	};
};

export const tourForRoute = (
	items: HubItem[],
	routeName: string | null | undefined,
): HubItem | null => {
	if (!routeName) return null;
	return (
		items.find(
			(item) =>
				item.available &&
				item.tour.triggers.some(
					(trigger) =>
						trigger.kind === 'first_visit' && trigger.routeName === routeName,
				),
		) ?? null
	);
};
