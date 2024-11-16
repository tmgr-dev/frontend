<template>
	<BaseLayout>
		<template #body>
			<!-- Header Section with Input and Stats -->
			<div class="w-full items-center justify-between pt-6 md:flex md:py-4">
				<!-- Input Form -->
				<div class="rounded-xl p-2 dark:bg-gray-900 md:w-4/6">
					<div
						class="flex items-center justify-between rounded-xl border px-2 dark:border-0 dark:bg-gray-800"
					>
						<TextField
							class="w-5/6 p-2"
							v-model="form.title"
							@keyup.enter="createTask"
							placeholder="Enter your routine (enter to add new task)"
						/>
						<Button
							class="my-1 ml-2 w-32 rounded-xl px-2 py-2 text-sm"
							@click="createTask"
						>
							<span class="relative">Add Task</span>
						</Button>
					</div>
				</div>

				<!-- Stats Section -->
				<ul class="m-4 flex justify-between md:mx-2">
					<li
						class="mx-2 flex w-28 flex-col rounded-xl border p-2 dark:border-0 dark:bg-gray-800"
					>
						<span>{{ tasks.length }}</span>
						<span class="text-gray-500">routines</span>
					</li>
					<li
						class="mx-2 flex w-28 flex-col rounded-xl border p-2 dark:border-0 dark:bg-gray-800"
					>
						<span>{{ completedCount }}</span>
						<span class="text-gray-500">completed</span>
					</li>
					<li
						class="mx-2 flex w-28 flex-col rounded-xl border p-2 dark:border-0 dark:bg-gray-800"
					>
						<span>{{ archivedTasksCount }}</span>
						<span class="text-gray-500">archived</span>
					</li>
				</ul>
			</div>

			<!-- Task List -->
			<div class="relative">
				<TransitionGroup name="task-list" tag="ul" class="w-full pt-4">
					<li
						v-for="task in tasks"
						:key="task.id"
						:data-task-id="task.id"
						class="task-item relative m-2 rounded-xl border px-5 py-3.5 dark:border-0 dark:bg-gray-800"
					>
						<div class="task-content flex w-full items-center justify-between">
							<!-- Left Side: Checkbox and Title -->
							<div class="flex items-center">
								<button
									@click="completeTask(task)"
									class="mr-3 rounded-full p-1 transition-colors hover:bg-gray-200 dark:hover:bg-gray-700"
									:class="{ 'text-green-500': task.status === 'completed' }"
								>
									<svg
										xmlns="http://www.w3.org/2000/svg"
										width="20"
										height="20"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										stroke-linecap="round"
										stroke-linejoin="round"
									>
										<path d="M20 6 9 17l-5-5" />
									</svg>
								</button>
								<span :class="{ 'line-through': task.status === 'completed' }">
									{{ task.title }}
								</span>
							</div>

							<!-- Right Side: Action Buttons -->
							<div class="flex items-center">
								<button
									v-if="task.status === 'completed'"
									@click="archiveTask(task)"
									class="mr-2 rounded-full p-1.5 text-gray-500 transition-colors hover:bg-gray-200 hover:text-blue-500 dark:hover:bg-gray-700"
								>
									<svg
										xmlns="http://www.w3.org/2000/svg"
										width="18"
										height="18"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										stroke-linecap="round"
										stroke-linejoin="round"
									>
										<path
											d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"
										/>
										<line x1="12" y1="11" x2="12" y2="17" />
										<line x1="9" y1="14" x2="15" y2="14" />
									</svg>
								</button>
								<button
									@click="deleteTask(task)"
									class="rounded-full p-1.5 text-gray-500 transition-colors hover:bg-gray-200 hover:text-red-500 dark:hover:bg-gray-700"
								>
									<svg
										xmlns="http://www.w3.org/2000/svg"
										width="18"
										height="18"
										viewBox="0 0 24 24"
										fill="none"
										stroke="currentColor"
										stroke-width="2"
										stroke-linecap="round"
										stroke-linejoin="round"
									>
										<path d="M3 6h18" />
										<path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />
										<path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
										<line x1="10" y1="11" x2="10" y2="17" />
										<line x1="14" y1="11" x2="14" y2="17" />
									</svg>
								</button>
							</div>
						</div>
					</li>
				</TransitionGroup>
			</div>
		</template>
	</BaseLayout>
</template>

<script setup lang="ts">
	import { computed, onMounted, ref } from 'vue';
	import TextField from '@/components/general/TextField.vue';
	import Button from '@/components/general/Button.vue';
	import {
		createDailyTask,
		getDailyTasks,
		deleteDailyTask,
		completeDailyTask,
		archiveDailyTask,
		getArchivedDailyTasksCount,
	} from '@/actions/tmgr/daily-tasks';
	import { Task } from '@/actions/tmgr/tasks';
	import { BreadcrumbItem, BreadcrumbLink } from '@/components/ui/breadcrumb';

	// Types
	interface ExtendedTask extends Task {
		isAnimating?: boolean;
	}

	// Refs
	const tasks = ref<ExtendedTask[]>([]);
	const archivedTasksCount = ref<Number>(0);
	const form = ref<Task>({ title: '', status: 'backlog' } as Task);

	// Computed
	const completedCount = computed(
		() => tasks.value.filter((task) => task.status === 'completed').length,
	);

	// Lifecycle
	onMounted(async () => {
		tasks.value = await getDailyTasks();
		archivedTasksCount.value = await getArchivedDailyTasksCount();
	});

	// Task Management Functions
	async function createTask() {
		if (!form.value.title?.trim()) return;

		try {
			const newTask = await createDailyTask(form.value);
			tasks.value = [newTask, ...tasks.value];
			form.value = { title: '', status: 'backlog' } as Task;
		} catch (error) {
			console.error('Failed to create task:', error);
		}
	}

	async function completeTask(task: ExtendedTask) {
		try {
			const result = await completeDailyTask(task.id);
			const taskIndex = tasks.value.findIndex((t) => t.id === task.id);
			if (taskIndex !== -1) {
				tasks.value[taskIndex] = { ...result, isAnimating: false };
			}
		} catch (error) {
			console.error('Failed to complete task:', error);
		}
	}

	async function deleteTask(task: ExtendedTask) {
		try {
			await deleteDailyTask(task.id);
			tasks.value = tasks.value.filter((t) => t.id !== task.id);
		} catch (error) {
			console.error('Failed to delete task:', error);
		}
	}

	async function archiveTask(task: ExtendedTask) {
		try {
			await archiveDailyTask(task.id);
			tasks.value = tasks.value.filter((t) => t.id !== task.id);
			archivedTasksCount.value = await getArchivedDailyTasksCount();
		} catch (error) {
			console.error('Failed to archive task:', error);
		}
	}
</script>

<style>
	.task-list-move,
	.task-list-enter-active,
	.task-list-leave-active {
		transition: all 0.3s ease;
	}

	.task-list-enter-from,
	.task-list-leave-to {
		opacity: 0;
		transform: translateY(30px);
	}

	.task-list-leave-active {
		position: absolute;
	}
</style>
