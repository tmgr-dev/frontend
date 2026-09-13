import { getMemberStats, getMemberTasks } from '@/actions/tmgr/dashboard';
import type {
	MemberStats,
	MemberTaskRow,
	MemberTasksTab,
	TeamActivityWindow,
} from '@/types/dashboard';
import {
	memberWindowFromQuery,
	tasksTabFromQuery,
} from '@/utils/dashboard/memberPageFormat';
import { createRequestSequence } from '@/utils/requestSequence';
import { computed, getCurrentInstance, onUnmounted, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';

const PER_PAGE = 20;

/**
 * State for the member page (#8988). The window and the tab live in the URL so a member's page can
 * be shared as it is being read; both are re-read from the route rather than mirrored in two places.
 */
export function useMemberPage(
	workspaceId: () => number | null,
	userId: () => number,
) {
	const route = useRoute();
	const router = useRouter();

	const stats = ref<MemberStats | null>(null);
	const statsLoading = ref(false);
	const notFound = ref(false);
	const statsError = ref<string | null>(null);
	const tasksError = ref<string | null>(null);
	const tasksLoaded = ref(false);
	const error = computed(() => statsError.value || tasksError.value);
	let attemptedTasksPage = 1;

	const tasks = ref<MemberTaskRow[]>([]);
	const tasksTotal = ref(0);
	const tasksLoading = ref(false);
	const tasksPage = ref(1);

	const window = computed(() =>
		memberWindowFromQuery(route.query.window as string | undefined),
	);
	const tab = computed(() =>
		tasksTabFromQuery(route.query.tab as string | undefined),
	);
	const hasMoreTasks = computed(() => tasks.value.length < tasksTotal.value);

	const setQuery = (patch: Record<string, string>) =>
		router.replace({ query: { ...route.query, ...patch } });

	const setWindow = (next: TeamActivityWindow) => setQuery({ window: next });
	const setTab = (next: MemberTasksTab) => setQuery({ tab: next });

	const statsRequests = createRequestSequence();
	const taskRequests = createRequestSequence();
	const context = () => `${workspaceId()}:${userId()}:${window.value}`;

	async function loadStats() {
		const token = statsRequests.begin();
		const key = context();
		const current = () => statsRequests.isCurrent(token) && context() === key;
		const wid = workspaceId();
		statsLoading.value = !!wid;
		if (!wid) return;
		notFound.value = false;
		statsError.value = null;
		try {
			const result = await getMemberStats(wid, userId(), window.value);
			if (!current()) return;
			if (result.success && result.data) stats.value = result.data;
			else if (
				(result.error?.details as { status?: number } | undefined)?.status ===
				404
			)
				notFound.value = true;
			else
				statsError.value =
					result.error?.message ?? 'Could not load this member';
		} catch (e) {
			if (current())
				statsError.value =
					e instanceof Error ? e.message : 'Could not load this member';
		} finally {
			if (current()) statsLoading.value = false;
		}
	}

	async function loadTasks(page = 1) {
		const token = taskRequests.begin();
		tasksError.value = null;
		attemptedTasksPage = page;
		const key = `${context()}:${tab.value}`;
		const current = () =>
			taskRequests.isCurrent(token) && `${context()}:${tab.value}` === key;
		const wid = workspaceId();
		tasksLoading.value = !!wid;
		if (!wid) return;
		try {
			const result = await getMemberTasks(wid, userId(), {
				tab: tab.value,
				window: window.value,
				page,
				perPage: PER_PAGE,
			});
			if (!current()) return;
			if (result.success && result.data) {
				tasksLoaded.value = true;
				const rows = result.data.data ?? [];
				tasks.value = page === 1 ? rows : [...tasks.value, ...rows];
				tasksTotal.value = result.data.total ?? tasks.value.length;
				tasksPage.value = page;
			} else
				tasksError.value =
					result.error?.message ?? 'Could not load member tasks';
		} catch (e) {
			if (current())
				tasksError.value =
					e instanceof Error ? e.message : 'Could not load member tasks';
		} finally {
			if (current()) tasksLoading.value = false;
		}
	}

	const loadMoreTasks = () => {
		if (!tasksLoading.value && hasMoreTasks.value)
			return loadTasks(tasksPage.value + 1);
	};

	async function refresh() {
		await Promise.all([loadStats(), loadTasks(1)]);
	}

	watch(
		[window, workspaceId, userId],
		() => {
			stats.value = null;
			tasks.value = [];
			tasksLoaded.value = false;
			tasksTotal.value = 0;
			tasksPage.value = 1;
			void refresh();
		},
		{ flush: 'sync' },
	);
	watch(
		tab,
		() => {
			tasks.value = [];
			tasksLoaded.value = false;
			tasksTotal.value = 0;
			tasksPage.value = 1;
			void loadTasks(1);
		},
		{ flush: 'sync' },
	);
	if (getCurrentInstance())
		onUnmounted(() => {
			statsRequests.dispose();
			taskRequests.dispose();
		});

	return {
		statsError,
		tasksError,
		tasksLoaded,
		retryTasks: () => loadTasks(attemptedTasksPage),
		loadStats,
		stats,
		statsLoading,
		notFound,
		error,
		tasks,
		tasksTotal,
		tasksLoading,
		hasMoreTasks,
		window,
		tab,
		setWindow,
		setTab,
		loadMoreTasks,
		refresh,
	};
}
