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

		<div
			v-else-if="error"
			class="rounded-card border border-line bg-surface p-6"
		>
			<p class="text-sm text-ink">{{ error }}</p>
			<button
				type="button"
				class="mt-3 rounded bg-brand px-3 py-1.5 text-sm text-white"
				@click="refresh"
			>
				Try again
			</button>
		</div>

		<template v-else>
			<MemberHeader
				:member="stats"
				:window="window"
				@window-change="setWindow"
				@task-click="openTask"
			/>

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
		</template>
	</div>
</template>

<script lang="ts">
	import { getActivityFeed } from '@/actions/tmgr/dashboard';
	import ActivityFeed from '@/components/dashboard/ActivityFeed.vue';
	import MemberHeader from '@/components/member/MemberHeader.vue';
	import MemberTasksPanel from '@/components/member/MemberTasksPanel.vue';
	import { useCurrentWorkspace } from '@/composable/useCurrentWorkspace';
	import { useMemberPage } from '@/composable/useMemberPage';
	import store from '@/store';
	import type { Activity } from '@/types/dashboard';
	import { computed, defineComponent, onMounted, ref, watch } from 'vue';
	import { useRoute } from 'vue-router';

	const ACTIVITIES_PER_PAGE = 20;

	export default defineComponent({
		name: 'MemberPage',
		components: { ActivityFeed, MemberHeader, MemberTasksPanel },
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
			const activitiesLoading = ref(false);
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
				const wid = currentWorkspaceId.value;

				if (!wid) return;

				activitiesLoading.value = true;

				const result = await getActivityFeed(wid, {
					page: nextPage,
					per_page: ACTIVITIES_PER_PAGE,
					filters: { user_id: userId.value } as never,
				});

				if (result.success && result.data) {
					const rows = (
						result.data as {
							data?: { data?: Activity[]; meta?: { total?: number } };
						}
					).data;
					activities.value =
						nextPage === 1
							? rows?.data ?? []
							: [...activities.value, ...(rows?.data ?? [])];
					activitiesTotal.value = rows?.meta?.total ?? activities.value.length;
					activitiesPage.value = nextPage;
				}

				activitiesLoading.value = false;
			}

			const loadMoreActivities = () => loadActivities(activitiesPage.value + 1);

			async function refresh() {
				await Promise.all([page.refresh(), loadActivities(1)]);
			}

			onMounted(refresh);
			watch(
				() => [workspaceId.value, userId.value],
				() => refresh(),
			);

			return {
				...page,
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
