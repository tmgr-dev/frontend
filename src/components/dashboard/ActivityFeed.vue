<template>
	<!-- CHANGES: Added min-h-96 to prevent CLS -->
	<div class="activity-feed min-h-96">
		<div class="activity-header mb-4 flex items-center justify-between">
			<h2 class="text-lg font-semibold text-ink">Recent Activity</h2>
			<div class="flex items-center space-x-2">
				<ActivityFilters
					v-model="filters"
					@update:model-value="handleFiltersChange"
					:workspace-users="workspaceUsers"
				/>
				<Button
					variant="ghost"
					size="sm"
					@click="refreshActivities"
					:disabled="loading"
					class="p-2"
					aria-label="Refresh activities"
				>
					<ArrowPathIcon :class="['h-4 w-4', loading && 'animate-spin']" />
				</Button>
			</div>
		</div>

		<div class="activity-list space-y-3">
			<!-- Loading skeleton -->
			<template v-if="loading && activities.length === 0">
				<ActivityItemSkeleton v-for="i in 5" :key="`skeleton-${i}`" />
			</template>

			<!-- Activities -->
			<template v-else-if="activities.length > 0">
				<ActivityItem
					v-for="activity in activities"
					:key="activity.id"
					:activity="activity"
					@click="handleActivityClick(activity)"
				/>

				<!-- Load more button -->
				<div v-if="hasMore" class="flex justify-center pt-4">
					<Button
						variant="outline"
						:loading="loadingMore"
						@click="handleLoadMore"
						:disabled="loadingMore"
					>
						<template v-if="loadingMore">
							<ArrowPathIcon class="mr-2 h-4 w-4 animate-spin" />
							Loading...
						</template>
						<template v-else> Load More Activities </template>
					</Button>
				</div>
			</template>

			<!-- Empty state -->
			<template v-else>
				<EmptyState
					icon="activity"
					title="No activities yet"
					description="Activities will appear here as team members work on tasks."
				/>
			</template>
		</div>

		<!-- Real-time indicator -->
		<div
			v-if="isRealTimeConnected"
			class="mt-4 flex items-center justify-center text-xs text-green-600 dark:text-green-400"
		>
			<div class="mr-2 h-2 w-2 animate-pulse rounded-full bg-green-500"></div>
			Live updates enabled
		</div>
	</div>
</template>

<script setup lang="ts">
	import EmptyState from '@/components/EmptyState.vue';
	import Button from '@/components/ui/button/Button.vue';
	import { usePusher } from '@/composable/usePusher';
	import type {
		Activity,
		ActivityFilters as ActivityFiltersType,
	} from '@/types/dashboard';
	import { ArrowPathIcon } from '@heroicons/vue/24/outline';
	import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
	import ActivityFilters from './ActivityFilters.vue';
	import ActivityItem from './ActivityItem.vue';
	import ActivityItemSkeleton from './ActivityItemSkeleton.vue';

	interface Props {
		activities: Activity[];
		loading: boolean;
		hasMore?: boolean;
		loadingMore?: boolean;
		workspaceId: number;
		workspaceUsers?: Array<{ id: number; name: string }>;
	}

	const props = withDefaults(defineProps<Props>(), {
		hasMore: false,
		loadingMore: false,
		workspaceUsers: () => [],
	});

	const emit = defineEmits<{
		'load-more': [];
		refresh: [];
		'filter-change': [filters: ActivityFiltersType];
		'activity-click': [activity: Activity];
	}>();

	const filters = ref<ActivityFiltersType>({
		type: 'all',
		user_id: 'all',
		limit: 20,
	});

	const { subscribe, unsubscribe, isConnected } = usePusher();

	const isRealTimeConnected = computed(() => isConnected.value);

	const handleFiltersChange = (newFilters: ActivityFiltersType) => {
		emit('filter-change', newFilters);
	};

	const handleLoadMore = () => {
		emit('load-more');
	};

	const refreshActivities = () => {
		emit('refresh');
	};

	const handleActivityClick = (activity: Activity) => {
		emit('activity-click', activity);
	};

	const handleNewActivity = (activity: Activity) => {
		// The parent component will handle adding the new activity
		// We could show a toast notification here
		console.log('New activity received:', activity);
	};

	const handleDashboardUpdate = (data: any) => {
		// Handle dashboard updates if needed
		console.log('Dashboard updated:', data);
	};

	// Watch for workspace changes to resubscribe
	watch(
		() => props.workspaceId,
		(newWorkspaceId, oldWorkspaceId) => {
			if (oldWorkspaceId) {
				unsubscribe(`App.Workspace.${oldWorkspaceId}`);
			}
			if (newWorkspaceId) {
				subscribe(`App.Workspace.${newWorkspaceId}`, {
					onActivityCreated: handleNewActivity,
					onDashboardUpdated: handleDashboardUpdate,
				});
			}
		},
		{ immediate: true },
	);

	onMounted(() => {
		if (props.workspaceId) {
			subscribe(`App.Workspace.${props.workspaceId}`, {
				onActivityCreated: handleNewActivity,
				onDashboardUpdated: handleDashboardUpdate,
			});
		}
	});

	onUnmounted(() => {
		if (props.workspaceId) {
			unsubscribe(`App.Workspace.${props.workspaceId}`);
		}
	});
</script>

<style scoped>
	.activity-feed {
		@apply rounded-card border border-line bg-surface p-6 shadow-tmgr-xs;
	}

	.activity-header {
		@apply mb-4 border-b border-line pb-4;
	}

	.activity-list {
		/* No inner scroll: the feed is the last block on the dashboard and grows with the page,
     bounded by its own "Load more" rather than by a scroll box (TM-235). */
		@apply min-h-64;
	}

	.activity-list > * + * {
		@apply border-t border-line;
	}

	/* Custom scrollbar */
	.activity-list::-webkit-scrollbar {
		@apply w-2;
	}

	.activity-list::-webkit-scrollbar-track {
		@apply rounded bg-gray-100 dark:bg-gray-700;
	}

	.activity-list::-webkit-scrollbar-thumb {
		@apply rounded bg-gray-300 hover:bg-gray-400 dark:bg-gray-600 dark:hover:bg-gray-500;
	}
</style>
