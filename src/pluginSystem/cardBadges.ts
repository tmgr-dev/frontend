import { reactive, watch } from 'vue';
import type { CardBadge } from './host';
import { pluginHost, pluginState } from './state';

const FIELDS = [
	'id',
	'title',
	'status_id',
	'project_category_id',
	'category_tasks_sequence_id',
	'priority',
	'common_time',
	'approximately_time',
	'start_time',
] as const;

/** What a badge provider sees of a card: plain task fields, no nested app objects. */
export const taskSnapshot = (task: Record<string, any>) => {
	const snapshot: Record<string, unknown> = {};
	for (const field of FIELDS) snapshot[field] = task[field] ?? null;
	snapshot.category = task.category?.code ? { code: task.category.code } : null;
	return snapshot as { id: number } & Record<string, unknown>;
};

/** Badges for every visible card, fetched in one batch per provider and refreshed when plugins ask. */
export const createCardBadgeFeed = (delayMs = 300) => {
	const badges = reactive<Record<number, CardBadge[]>>({});
	let tasks: ReturnType<typeof taskSnapshot>[] = [];
	let timer: ReturnType<typeof setTimeout> | undefined;
	let request = 0;

	const refresh = () => {
		clearTimeout(timer);
		timer = setTimeout(async () => {
			const current = ++request;
			const host = pluginHost();
			const next =
				host && tasks.length ? await host.badges(tasks).catch(() => ({})) : {};
			if (current !== request) return;
			Object.keys(badges).forEach((key) => delete badges[Number(key)]);
			Object.assign(badges, next);
		}, delayMs);
	};

	const stop = watch(
		() =>
			`${pluginState.revision}|${Object.values(pluginState.plugins)
				.map((p) => p.status)
				.join()}`,
		refresh,
	);

	return {
		badges,
		update(next: Record<string, any>[]) {
			tasks = next.map(taskSnapshot);
			if (pluginHost()) refresh();
		},
		dispose() {
			stop();
			clearTimeout(timer);
			request++;
		},
	};
};
