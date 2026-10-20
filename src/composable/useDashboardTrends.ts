import { getWorkspaceTimeDaily } from '@/actions/tmgr/timeDaily';
import { useFeatureToggles } from '@/composable/useFeatureToggles';
import type {
	DashboardPeriod,
	DetailedLoadingState,
	HeatmapData,
	TeamActivityWindow,
	TeamMemberActivity,
	TimeDailyDay,
} from '@/types/dashboard';
import {
	activeMembersCount,
	activeMembersPerDay,
	categoryTotals,
	countsForDays,
	deltaPercent,
	periodLength,
	sumValues,
	trackedSecondsForDays,
	utcDayKeys,
} from '@/utils/dashboard/periodStats';
import { createRequestSequence } from '@/utils/requestSequence';
import type { Ref } from 'vue';
import {
	computed,
	getCurrentInstance,
	onUnmounted,
	ref,
	shallowRef,
	watch,
} from 'vue';

type TimeState = 'off' | 'loading' | 'ready' | 'failed';

/**
 * Period numbers for the dashboard KPI row, chart and categories, derived from the
 * heatmap, team-activity and /time/daily. /time/daily failures stay silent.
 */
export function useDashboardTrends(sources: {
	workspaceId: Ref<number>;
	period: Ref<DashboardPeriod>;
	heatmapData: Ref<HeatmapData | null>;
	teamActivity: Ref<TeamMemberActivity | null>;
	teamActivityWindow: Ref<TeamActivityWindow>;
	teamState: Ref<DetailedLoadingState>;
}) {
	const {
		workspaceId,
		period,
		heatmapData,
		teamActivity,
		teamActivityWindow,
		teamState,
	} = sources;
	const { showSurface, isFeatureEnabled } = useFeatureToggles();

	const today = ref(new Date());
	const length = computed(() => periodLength(period.value));
	const keys = computed(() => utcDayKeys(today.value, length.value));
	const previousKeys = computed(() =>
		utcDayKeys(today.value, length.value, length.value),
	);

	// The widget can switch team-activity to another window; keep the last load of each window.
	const teamSnapshots = shallowRef<
		Partial<Record<TeamActivityWindow, TeamMemberActivity>>
	>({});
	watch(
		[teamActivity, teamActivityWindow, () => teamState.value.status],
		() => {
			if (teamState.value.status === 'success' && teamActivity.value)
				teamSnapshots.value = {
					...teamSnapshots.value,
					[teamActivityWindow.value]: teamActivity.value,
				};
		},
		{ immediate: true },
	);
	const periodTeam = computed(() => teamSnapshots.value[period.value] ?? null);

	const showTracked = computed(() => showSurface('timer.stats'));
	const timeEnabled = computed(
		() =>
			isFeatureEnabled('dashboard') &&
			showTracked.value &&
			isFeatureEnabled('exports'),
	);
	const loadedTime = shallowRef<{
		range: string;
		days: TimeDailyDay[] | null;
		previous: TimeDailyDay[] | null;
	} | null>(null);
	const range = computed(
		() => `${keys.value[0]}..${keys.value[keys.value.length - 1]}`,
	);
	const timeDays = computed(() =>
		loadedTime.value?.range === range.value ? loadedTime.value.days : null,
	);
	const timePrevious = computed(() =>
		loadedTime.value?.range === range.value ? loadedTime.value.previous : null,
	);
	const timeState = ref<TimeState>('off');
	const timeRequests = createRequestSequence();

	const loadTime = async (force = false): Promise<void> => {
		const request = timeRequests.begin();
		today.value = new Date();
		if (!timeEnabled.value || !workspaceId.value) {
			timeState.value = 'off';
			loadedTime.value = null;
			return;
		}
		const current = keys.value;
		const previous = previousKeys.value;
		const requested = range.value;
		timeState.value = 'loading';
		const [now, before] = await Promise.allSettled([
			getWorkspaceTimeDaily(
				workspaceId.value,
				current[0],
				current[current.length - 1],
				!force,
			),
			getWorkspaceTimeDaily(
				workspaceId.value,
				previous[0],
				previous[previous.length - 1],
				!force,
			),
		]);
		if (!timeRequests.isCurrent(request)) return;
		loadedTime.value = {
			range: requested,
			days: now.status === 'fulfilled' ? now.value : null,
			previous: before.status === 'fulfilled' ? before.value : null,
		};
		timeState.value = now.status === 'fulfilled' ? 'ready' : 'failed';
	};

	watch([period, timeEnabled], () => loadTime());
	watch(workspaceId, () => {
		teamSnapshots.value = {};
		timeRequests.begin();
		loadedTime.value = null;
		timeState.value = 'off';
	});

	const teamPending = computed(
		() =>
			!periodTeam.value &&
			['idle', 'loading'].includes(teamState.value.status ?? 'idle'),
	);
	const teamFailed = computed(() => !periodTeam.value && !teamPending.value);

	const activity = computed(() => {
		const contributions = heatmapData.value?.contributions;
		if (!contributions) return null;
		const values = countsForDays(contributions, keys.value);
		const total = sumValues(values);
		const previous = sumValues(
			countsForDays(contributions, previousKeys.value),
		);
		return { values, total, delta: deltaPercent(total, previous) };
	});

	const members = computed(() => {
		const team = periodTeam.value;
		if (!team) return null;
		return {
			values: activeMembersPerDay(team.members ?? [], keys.value),
			count: activeMembersCount(team.members ?? [], keys.value),
			total: team.total_members ?? team.members?.length ?? 0,
		};
	});

	const completed = computed(() => periodTeam.value?.done_count ?? null);

	const tracked = computed(() => {
		if (timeDays.value) {
			const perDay = trackedSecondsForDays(timeDays.value, keys.value);
			const seconds = sumValues(perDay);
			const previous = timePrevious.value
				? sumValues(
						trackedSecondsForDays(timePrevious.value, previousKeys.value),
				  )
				: null;
			return {
				seconds,
				perDay,
				delta: previous === null ? null : deltaPercent(seconds, previous),
			};
		}
		const seconds = periodTeam.value?.tracked_seconds;
		return {
			seconds: seconds === undefined ? null : Number(seconds) || 0,
			perDay: null,
			delta: null,
		};
	});
	const trackedLoading = computed(
		() =>
			timeState.value === 'loading' || (!timeDays.value && teamPending.value),
	);
	const trackedFailed = computed(
		() => !timeDays.value && timeState.value !== 'loading' && teamFailed.value,
	);

	const categories = computed(() =>
		timeDays.value ? categoryTotals(timeDays.value, keys.value) : null,
	);

	if (getCurrentInstance()) onUnmounted(() => timeRequests.dispose());

	return {
		keys,
		showTracked,
		timeState,
		teamPending,
		teamFailed,
		activity,
		members,
		completed,
		tracked,
		trackedLoading,
		trackedFailed,
		categories,
		loadTime,
	};
}
