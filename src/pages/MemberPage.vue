<template>
	<div class="mx-auto w-full max-w-6xl space-y-4 p-4">
		<nav class="text-xs text-ink-subtle" aria-label="Breadcrumb">
			<router-link :to="dashboardLink" class="hover:text-ink"
				>Dashboard</router-link
			>
			<span> / </span>
			<span class="text-ink">{{ stats?.name ?? 'Member' }}</span>
		</nav>

		<p
			v-if="notFound"
			class="rounded-card border border-line bg-surface p-6 text-sm text-ink-subtle"
		>
			This user is not a member of this workspace.
		</p>

		<div v-else class="space-y-4">
			<AsyncContent
				:pending="statsLoading"
				:has-data="!!stats"
				:error="statsError"
				:retry="loadStats"
				label="Loading member"
			>
				<MemberHeader
					:member="stats"
					:window="window"
					@window-change="setWindow"
					@task-click="openTask"
				/>
			</AsyncContent>
			<AsyncContent
				:pending="tasksLoading"
				:loaded="tasksLoaded"
				:error="tasksError"
				:retry="retryTasks"
				label="Loading member tasks"
			>
				<MemberTasksPanel
					:tasks="tasks"
					:tab="tab"
					:total="tasksTotal"
					:loading="tasksLoading"
					:has-more="hasMoreTasks"
					@tab-change="setTab"
					@task-click="openTask"
					@load-more="loadMoreTasks"
				/>
			</AsyncContent>
			<AsyncContent
				:pending="activitiesLoading"
				:loaded="activitiesLoaded"
				:error="activitiesError"
				:retry="retryActivities"
				label="Loading activity"
			>
				<ActivityFeed
					v-if="workspaceId"
					:activities="activities"
					:loading="activitiesLoading"
					:has-more="activitiesHasMore"
					:workspace-id="workspaceId"
					@load-more="loadMoreActivities"
					@refresh="loadActivities"
					@activity-click="handleActivityClick"
				/>
			</AsyncContent>
		</div>
	</div>
</template>

<script lang="ts">
	import { getActivityFeed } from '@/actions/tmgr/dashboard';
	import AsyncContent from '@/components/async/AsyncContent.vue';
	import ActivityFeed from '@/components/dashboard/ActivityFeed.vue';
	import MemberHeader from '@/components/member/MemberHeader.vue';
	import MemberTasksPanel from '@/components/member/MemberTasksPanel.vue';
	import { useCurrentWorkspace } from '@/composable/useCurrentWorkspace';
	import { useMemberPage } from '@/composable/useMemberPage';
	import store from '@/store';
	import type { Activity } from '@/types/dashboard';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
		watch,
	} from 'vue';
	import { useRoute } from 'vue-router';

	const ACTIVITIES_PER_PAGE = 20;

	export default defineComponent({
		name: 'MemberPage',
		components: { AsyncContent, ActivityFeed, MemberHeader, MemberTasksPanel },
		setup() {
			const route = useRoute();
			const { currentWorkspaceId, currentWorkspaceCode } =
				useCurrentWorkspace();

			const userId = computed(() => Number(route.params.user_id));
			const workspaceId = computed(() => currentWorkspaceId.value);

			const page = useMemberPage(
				() => currentWorkspaceId.value,
				() => userId.value,
			);

			const activities = ref<Activity[]>([]);
			const activitiesLoading = ref(true);
			const activitiesLoaded = ref(false),
				activitiesError = ref<string | null>(null);
			let activityRequest = 0,
				activityAttempt = 1,
				disposed = false;
			onBeforeUnmount(() => {
				disposed = true;
				++activityRequest;
			});
			const activitiesPage = ref(1);
			const activitiesTotal = ref(0);
			const activitiesHasMore = computed(
				() => activities.value.length < activitiesTotal.value,
			);

			const dashboardLink = computed(() => ({
				path: `/${currentWorkspaceCode.value}/dashboard`,
			}));

			const openTask = (taskId: number) =>
				store.commit('setCurrentTaskIdForModal', taskId);

			const handleActivityClick = (activity: Activity) => {
				if (activity.subject_id) openTask(activity.subject_id);
			};

			async function loadActivities(nextPage = 1) {
				const wid = currentWorkspaceId.value,
					uid = userId.value;
				const request = ++activityRequest;
				const current = () =>
					!disposed &&
					request === activityRequest &&
					wid === currentWorkspaceId.value &&
					uid === userId.value;
				if (!wid) {
					activitiesLoading.value = false;
					activitiesLoaded.value = true;
					return;
				}
				activityAttempt = nextPage;
				activitiesError.value = null;

				activitiesLoading.value = true;

				try {
					const result = await getActivityFeed(wid, {
						page: nextPage,
						per_page: ACTIVITIES_PER_PAGE,
						filters: { user_id: userId.value } as never,
					});

					if (!current()) return;
					if (result.success && result.data) {
						activitiesLoaded.value = true;
						const rows = (
							result.data as {
								data?: { data?: Activity[]; meta?: { total?: number } };
							}
						).data;
						activities.value =
							nextPage === 1
								? rows?.data ?? []
								: [...activities.value, ...(rows?.data ?? [])];
						activitiesTotal.value =
							rows?.meta?.total ?? activities.value.length;
						activitiesPage.value = nextPage;
					} else {
						activitiesError.value =
							result.error?.message || 'Could not load activity.';
					}
				} catch {
					if (current()) activitiesError.value = 'Could not load activity.';
				} finally {
					if (current()) activitiesLoading.value = false;
				}
			}

			const loadMoreActivities = () => {
				if (!activitiesLoading.value)
					return loadActivities(activitiesPage.value + 1);
			};
			const retryActivities = () => loadActivities(activityAttempt);

			async function refresh() {
				await Promise.all([page.refresh(), loadActivities(1)]);
			}

			onMounted(refresh);
			watch(
				() => [workspaceId.value, userId.value],
				() => {
					activities.value = [];
					activitiesLoaded.value = false;
					activitiesTotal.value = 0;
					void loadActivities(1);
				},
			);

			return {
				...page,
				activitiesLoaded,
				activitiesError,
				retryActivities,
				workspaceId,
				dashboardLink,
				activities,
				activitiesLoading,
				activitiesHasMore,
				loadActivities,
				loadMoreActivities,
				handleActivityClick,
				openTask,
				refresh,
			};
		},
	});
</script>
