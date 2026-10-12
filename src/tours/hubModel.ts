import type { HubItem } from './useTours';
import type { TourGroup } from './types';

export const GROUP_TITLES: Record<TourGroup, string> = {
	start: 'Getting started',
	tasks: 'Tasks & board',
	time: 'Time',
	agents: 'Agents & MCP',
	personas: 'Personas',
	pages: 'Pages & context',
	graph: 'Graph & map',
	routines: 'Routines',
	workspace: 'Workspaces & settings',
	desktop: 'Desktop',
};

const GROUP_ORDER = Object.keys(GROUP_TITLES) as TourGroup[];

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
		? `Turn on ${label} in Settings -> Modules`
		: `Ask the workspace owner to turn on ${label}`;
};

const rowFor = (item: HubItem, context: HubContext): HubRow | null => {
	const { tour, state } = item;
	const base = {
		id: tour.id,
		title: tour.title,
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
			progress: `step ${position} of ${tour.steps.length}`,
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
			title: GROUP_TITLES[group],
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
