import { computed, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { getMemberStats, getMemberTasks } from '@/actions/tmgr/dashboard';
import {
  memberWindowFromQuery,
  tasksTabFromQuery,
} from '@/utils/dashboard/memberPageFormat';
import type {
  MemberStats,
  MemberTaskRow,
  MemberTasksTab,
  TeamActivityWindow,
} from '@/types/dashboard';

const PER_PAGE = 20;

/**
 * State for the member page (#8988). The window and the tab live in the URL so a member's page can
 * be shared as it is being read; both are re-read from the route rather than mirrored in two places.
 */
export function useMemberPage(workspaceId: () => number | null, userId: () => number) {
	const route = useRoute();
	const router = useRouter();

	const stats = ref<MemberStats | null>(null);
	const statsLoading = ref(false);
	const notFound = ref(false);
	const error = ref<string | null>(null);

	const tasks = ref<MemberTaskRow[]>([]);
	const tasksTotal = ref(0);
	const tasksLoading = ref(false);
	const tasksPage = ref(1);

	const window = computed(() => memberWindowFromQuery(route.query.window as string | undefined));
	const tab = computed(() => tasksTabFromQuery(route.query.tab as string | undefined));
	const hasMoreTasks = computed(() => tasks.value.length < tasksTotal.value);

	const setQuery = (patch: Record<string, string>) =>
		router.replace({ query: { ...route.query, ...patch } });

	const setWindow = (next: TeamActivityWindow) => setQuery({ window: next });
	const setTab = (next: MemberTasksTab) => setQuery({ tab: next });

	async function loadStats() {
		const wid = workspaceId();

		if (!wid) {
			return;
		}

		statsLoading.value = true;
		notFound.value = false;
		error.value = null;

		const result = await getMemberStats(wid, userId(), window.value);

		if (result.success && result.data) {
			stats.value = result.data;
		} else if ((result.error?.details as { status?: number } | undefined)?.status === 404) {
			notFound.value = true;
		} else {
			error.value = result.error?.message ?? 'Could not load this member';
		}

		statsLoading.value = false;
	}

	async function loadTasks(page = 1) {
		const wid = workspaceId();

		if (!wid) {
			return;
		}

		tasksLoading.value = true;

		const result = await getMemberTasks(wid, userId(), {
			tab: tab.value,
			window: window.value,
			page,
			perPage: PER_PAGE,
		});

		if (result.success && result.data) {
			tasks.value = page === 1 ? result.data.data : [...tasks.value, ...result.data.data];
			tasksTotal.value = result.data.total;
			tasksPage.value = page;
		}

		tasksLoading.value = false;
	}

	const loadMoreTasks = () => loadTasks(tasksPage.value + 1);

	async function refresh() {
		await Promise.all([loadStats(), loadTasks(1)]);
	}

	watch([window, () => userId()], () => refresh());
	watch(tab, () => loadTasks(1));

	return {
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
