import { getUserHeatmap } from '@/actions/tmgr/dashboard';
import type { HeatmapData } from '@/types/dashboard';
import { requestCache } from '@/utils/requestCache';
import { createRequestSequence } from '@/utils/requestSequence';
import type { Ref } from 'vue';
import { computed, onScopeDispose, ref, watch } from 'vue';

export const LAST_12_MONTHS = 'last_12_months';
export type HeatmapRangeChoice = typeof LAST_12_MONTHS | number;

const parseChoice = (raw: string | null): HeatmapRangeChoice =>
	raw && /^\d{4}$/.test(raw) ? Number(raw) : LAST_12_MONTHS;

export const readStoredRange = (key: string | null): HeatmapRangeChoice => {
	if (!key) return LAST_12_MONTHS;
	try {
		return parseChoice(localStorage.getItem(key));
	} catch {
		return LAST_12_MONTHS;
	}
};

export const writeStoredRange = (key: string, choice: HeatmapRangeChoice) => {
	try {
		localStorage.setItem(key, String(choice));
	} catch {
		// Private mode or quota: the choice just isn't remembered.
	}
};

/**
 * Card-only year view: the default `heatmapData` is never replaced, so KPI trends keep the rolling data.
 */
export function useHeatmapRange(options: {
	heatmapData: Ref<HeatmapData | null>;
	workspaceId: () => number | null | undefined;
	userId: () => number | null | undefined;
}) {
	const storageKey = computed(() => {
		const user = options.userId();
		const workspace = options.workspaceId();
		return user && workspace
			? `tmgr:dashboard:heatmap-range:${user}:${workspace}`
			: null;
	});
	const stored = ref<HeatmapRangeChoice>(readStoredRange(storageKey.value));
	watch(storageKey, (key) => {
		stored.value = readStoredRange(key);
	});

	// Old backend: no `years`, so no select and no year request.
	const years = computed(() => {
		const list = options.heatmapData.value?.years;
		return Array.isArray(list) && list.length ? list : null;
	});
	const year = computed(() => {
		const choice = stored.value;
		return typeof choice === 'number' && years.value?.includes(choice)
			? choice
			: null;
	});

	const snapshotKey = (y: number, workspace: unknown) =>
		requestCache.generateKey('dashboard-snapshot-heatmap-year', {
			workspace,
			year: y,
		});

	const yearData = ref<HeatmapData | null>(null);
	const loading = ref(false);
	const error = ref(false);
	const sequence = createRequestSequence();
	onScopeDispose(sequence.dispose);

	const reload = async () => {
		const token = sequence.begin();
		const y = year.value;
		const workspace = options.workspaceId();
		error.value = false;
		loading.value = y !== null && !!workspace;
		if (!loading.value) return;
		const result = await getUserHeatmap(workspace!, {
			range: 'year',
			year: y!,
		});
		if (!sequence.isCurrent(token)) return;
		loading.value = false;
		const data = result.success ? result.data : null;
		if (data?.from && data.to) {
			yearData.value = data;
			requestCache.set(snapshotKey(y!, workspace), data);
		} else if (data) {
			select(LAST_12_MONTHS);
		} else {
			error.value = true;
		}
	};

	watch(
		[year, options.workspaceId],
		([y, workspace]) => {
			yearData.value =
				y === null ? null : requestCache.get(snapshotKey(y, workspace));
			reload();
		},
		{ immediate: true },
	);

	const select = (value: string) => {
		const choice = parseChoice(value);
		stored.value = choice;
		if (storageKey.value) writeStoredRange(storageKey.value, choice);
	};

	return {
		years,
		year,
		data: computed(() =>
			year.value === null ? options.heatmapData.value : yearData.value,
		),
		loading,
		error,
		selectValue: computed(() =>
			year.value === null ? LAST_12_MONTHS : String(year.value),
		),
		select,
		reload,
	};
}
