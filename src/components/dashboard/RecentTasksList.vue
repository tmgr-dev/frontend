<template>
	<div class="recent-tasks-list">
		<div class="recent-tasks-header mb-4 flex items-center justify-between">
			<h3 class="text-lg font-semibold text-gray-900 dark:text-white">
				Recent Tasks
			</h3>
			<div class="flex items-center space-x-2">
				<Button
					variant="ghost"
					size="sm"
					@click="refreshTasks"
					:disabled="loading"
					class="p-2"
					aria-label="Refresh recent tasks"
				>
					<ArrowPathIcon :class="['h-4 w-4', loading && 'animate-spin']" />
				</Button>
				<Button variant="outline" size="sm" @click="viewAllTasks">
					View All
				</Button>
			</div>
		</div>

		<div class="recent-tasks-content">
			<!-- Loading skeleton -->
			<template v-if="loading && tasks.length === 0">
				<div class="space-y-3">
					<RecentTaskSkeleton v-for="i in 5" :key="`skeleton-${i}`" />
				</div>
			</template>

			<!-- Tasks list -->
			<template v-else-if="tasks.length > 0">
				<div class="space-y-3">
					<RecentTaskItem
						v-for="task in tasks"
						:key="task.id"
						:task="task"
						@click="handleTaskClick(task)"
						@timer-toggle="handleTimerToggle(task)"
						@status-change="handleStatusChange(task, $event)"
					/>
				</div>
			</template>

			<!-- Empty state -->
			<template v-else>
				<EmptyState
					icon="task"
					title="No recent tasks"
					description="Recent task updates will appear here."
					:action="{
						label: 'Create Task',
						onClick: createNewTask,
					}"
				/>
			</template>
		</div>
	</div>
</template>

<script setup lang="ts">
	import EmptyState from '@/components/EmptyState.vue';
	import Button from '@/components/ui/button/Button.vue';
	import type { RecentTask } from '@/types/dashboard';
	import { ArrowPathIcon } from '@heroicons/vue/24/outline';
	import { useRouter } from 'vue-router';
	import RecentTaskItem from './RecentTaskItem.vue';
	import RecentTaskSkeleton from './RecentTaskSkeleton.vue';

	interface Props {
		tasks: RecentTask[];
		loading: boolean;
	}

	const props = defineProps<Props>();

	const emit = defineEmits<{
		refresh: [];
		'task-click': [task: RecentTask];
		'timer-toggle': [task: RecentTask];
		'status-change': [task: RecentTask, statusId: number];
		'create-task': [];
	}>();

	const router = useRouter();

	const refreshTasks = () => {
		emit('refresh');
	};

	const viewAllTasks = () => {
		router.push('/tasks');
	};

	const createNewTask = () => {
		emit('create-task');
		router.push('/tasks/new');
	};

	const handleTaskClick = (task: RecentTask) => {
		emit('task-click', task);
	};

	const handleTimerToggle = (task: RecentTask) => {
		emit('timer-toggle', task);
	};

	const handleStatusChange = (task: RecentTask, statusId: number) => {
		emit('status-change', task, statusId);
	};
</script>

<style scoped>
	.recent-tasks-list {
		@apply rounded-lg border border-gray-200 bg-white p-6 dark:border-gray-700 dark:bg-gray-800;
	}

	.recent-tasks-header {
		@apply mb-4 border-b border-gray-200 pb-4 dark:border-gray-700;
	}

	.recent-tasks-content {
		@apply max-h-80 overflow-y-auto;
	}

	/* Custom scrollbar */
	.recent-tasks-content::-webkit-scrollbar {
		@apply w-2;
	}

	.recent-tasks-content::-webkit-scrollbar-track {
		@apply rounded bg-gray-100 dark:bg-gray-700;
	}

	.recent-tasks-content::-webkit-scrollbar-thumb {
		@apply rounded bg-gray-300 hover:bg-gray-400 dark:bg-gray-600 dark:hover:bg-gray-500;
	}
</style>
