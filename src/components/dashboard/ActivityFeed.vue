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

		<div
			v-if="selectedDate"
			class="mb-3 flex items-center gap-2 text-sm text-ink-muted"
		>
			<span aria-live="polite">{{ dayFilterText }}</span>
			<Button
				variant="ghost"
				size="sm"
				class="h-6 w-6 p-0"
				aria-label="Clear day filter"
				@click="emit('clear-date')"
			>
				<XMarkIcon class="h-4 w-4" />
			</Button>
		</div>

		<div v-if="hasNonUserActors" class="mb-3 flex gap-1">
			<button
				v-for="option in authorFilterOptions"
				:key="option.value"
				type="button"
				:class="[
					'rounded-full px-2 py-0.5 text-2xs font-medium transition-colors',
					authorFilter === option.value
						? 'bg-brand-bg text-ink'
						: 'bg-surface-sunken text-ink-muted hover:bg-surface-hover',
				]"
				@click="authorFilter = option.value"
			>
				{{ option.label }}
			</button>
		</div>

		<div class="activity-list space-y-3">
			<!-- Loading skeleton -->
			<template v-if="loading && activities.length === 0">
				<ActivityItemSkeleton v-for="i in 5" :key="`skeleton-${i}`" />
			</template>

			<p
				v-else-if="selectedDate && error && activities.length === 0"
				role="alert"
				class="py-4 text-center text-sm text-status-fix-fg"
			>
				{{ error.message || "Couldn't load activities for this day" }}
			</p>

			<!-- Activities -->
			<template v-else-if="visibleActivities.length > 0">
				<ActivityItem
					v-for="activity in visibleActivities"
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

			<!-- Filtered to nothing -->
			<template v-else-if="activities.length > 0">
				<p class="py-4 text-center text-sm text-ink-subtle">
					No activities match this filter
				</p>
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
			class="mt-4 flex items-center justify-center text-xs text-status-done-fg"
		>
			<div class="mr-2 h-2 w-2 animate-pulse rounded-full bg-status-done"></div>
			Live updates enabled
		</div>
	</div>
</template>

<script setup lang="ts">
	import EmptyState from '@/components/EmptyState.vue';
	import Button from '@/components/ui/button/Button.vue';
	import type {
		ActionError,
		Activity,
		ActivityFilters as ActivityFiltersType,
	} from '@/types/dashboard';
	import {
		activityCountLabel,
		formatDayLabel,
	} from '@/utils/dashboard/activityDayFilter';
	import { type AuthorFilter, matchesAuthorFilter } from '@/utils/personas';
	import { ArrowPathIcon, XMarkIcon } from '@heroicons/vue/24/outline';
	import { computed, ref } from 'vue';
	import ActivityFilters from './ActivityFilters.vue';
	import ActivityItem from './ActivityItem.vue';
	import ActivityItemSkeleton from './ActivityItemSkeleton.vue';

	interface Props {
		activities: Activity[];
		loading: boolean;
		hasMore?: boolean;
		loadingMore?: boolean;
		workspaceId: number;
		connected?: boolean;
		workspaceUsers?: Array<{ id: number; name: string; has_avatar?: boolean }>;
		selectedDate?: string | null;
		totalItems?: number;
		error?: ActionError | null;
	}

	const props = withDefaults(defineProps<Props>(), {
		hasMore: false,
		loadingMore: false,
		workspaceUsers: () => [],
		selectedDate: null,
		totalItems: 0,
		error: null,
	});

	const emit = defineEmits<{
		'load-more': [];
		refresh: [];
		'filter-change': [filters: ActivityFiltersType];
		'activity-click': [activity: Activity];
		'clear-date': [];
	}>();

	const dayFilterText = computed(() => {
		if (!props.selectedDate) return '';
		const day = formatDayLabel(props.selectedDate);
		return props.loading || (props.error && props.activities.length === 0)
			? `Activities on ${day}`
			: `${activityCountLabel(props.totalItems)} on ${day}`;
	});

	const filters = ref<ActivityFiltersType>({
		type: 'all',
		user_id: 'all',
		limit: 20,
	});

	const isRealTimeConnected = computed(() => props.connected ?? false);

	const authorFilter = ref<AuthorFilter>('all');
	const authorFilterOptions: { value: AuthorFilter; label: string }[] = [
		{ value: 'all', label: 'All' },
		{ value: 'people', label: 'People' },
		{ value: 'personas', label: 'Personas' },
		{ value: 'plugins', label: 'Plugins' },
	];
	const hasNonUserActors = computed(() =>
		props.activities.some((a) => a.actor && a.actor.kind !== 'user'),
	);
	const visibleActivities = computed(() =>
		props.activities.filter((a) => matchesAuthorFilter(a, authorFilter.value)),
	);

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
</script>

<style scoped>
	@reference '@/assets/styles/tailwind.css';

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
		@apply rounded bg-surface-sunken;
	}

	.activity-list::-webkit-scrollbar-thumb {
		@apply rounded bg-line-strong hover:bg-ink-faint;
	}
</style>
