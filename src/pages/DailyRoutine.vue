<template>
	<FeatureGate
		feature-key="daily_routines"
		title="Daily Routines"
		description="Create recurring tasks and track your daily habits. Build consistent routines and monitor your progress over time."
		:icon="CalendarCheck"
	>
		<template #preview>
			<DailyRoutinesPreview />
		</template>

	<BaseLayout>
		<template #body>
			<div class="relative flex h-full overflow-hidden mb-10">
				<div class="w-full">
					<!-- Header Section with Input and Stats -->
					<div class="w-full items-center justify-between pt-6 md:flex md:gap-4 md:py-4">
						<!-- Input Form -->
						<div class="md:flex-1">
							<div class="flex items-center gap-2 rounded-card border border-line bg-surface px-2 py-1.5 shadow-tmgr-xs">
								<TextField
									class="flex-1"
									v-model="form.title"
									@keyup.enter="createTask"
									placeholder="Enter your routine (enter to add new task)"
								/>
								<button
									type="button"
									class="h-9 shrink-0 rounded-pill bg-brand px-4 text-sm font-semibold text-white shadow-tmgr-xs transition-colors hover:bg-brand-hover"
									@click="createTask"
								>
									Add Task
								</button>
							</div>
						</div>

						<!-- Stats Section -->
						<ul class="mt-4 grid grid-cols-3 gap-2 md:mt-0 md:flex md:shrink-0 md:gap-3">
							<li class="flex w-full flex-col items-start gap-0.5 rounded-card border border-line bg-surface px-3 py-2 shadow-tmgr-xs md:w-24">
								<span class="text-lg font-semibold tabular-nums text-ink">{{ allTasksCount }}</span>
								<span class="text-2xs uppercase tracking-wide text-ink-subtle">routines</span>
							</li>
							<li class="flex w-full flex-col items-start gap-0.5 rounded-card border border-line bg-surface px-3 py-2 shadow-tmgr-xs md:w-24">
								<span class="text-lg font-semibold tabular-nums text-status-done-fg">{{ completedCount }}</span>
								<span class="text-2xs uppercase tracking-wide text-ink-subtle">completed</span>
							</li>
							<li class="flex w-full flex-col items-start gap-0.5 rounded-card border border-line bg-surface px-3 py-2 shadow-tmgr-xs md:w-24">
								<span class="text-lg font-semibold tabular-nums text-ink-muted">{{ archivedTasksCount }}</span>
								<span class="text-2xs uppercase tracking-wide text-ink-subtle">archived</span>
							</li>
						</ul>
					</div>

					<div class="flex gap-3 pt-4">
						<!-- Task List -->
						<div
							:class="[
								'transition-all duration-300 ease-in-out min-h-64',
								selectedTask ? 'w-1/2' : 'w-full'
							]"
						>
							<!-- Loading skeleton -->
							<div v-if="isLoading" class="space-y-2">
								<SkeletonListItem v-for="n in 5" :key="n" class="rounded-card border border-line bg-surface" :show-avatar="false" />
							</div>

							<TransitionGroup v-else name="task-list" tag="ul" class="">
								<li
									v-for="task in tasks"
									:key="task.id"
									:data-task-id="task.id"
									@click="selectTask(task)"
									:class="[
										'task-item group relative mb-2 cursor-pointer rounded-card border bg-surface px-5 py-3.5 shadow-tmgr-xs transition-all',
										selectedTask?.id == task.id && !selectedTask?.is_recurring
											? 'border-brand bg-brand-bg shadow-tmgr-md'
											: 'border-line hover:border-line-strong hover:shadow-tmgr-md',
									]"
								>
									<div class="task-content flex w-full items-center justify-between">
										<!-- Left Side: Checkbox and Title -->
										<div class="flex items-center">
											<button
												@click.stop="completeTask(task)"
												:class="[
													'mr-3 flex h-7 w-7 items-center justify-center rounded-pill border transition-colors',
													task.status?.toLowerCase() === 'completed'
														? 'border-status-done bg-status-done text-white'
														: 'border-line text-ink-subtle hover:border-status-done hover:text-status-done',
												]"
											>
												<svg
													xmlns="http://www.w3.org/2000/svg"
													width="16"
													height="16"
													viewBox="0 0 24 24"
													fill="none"
													stroke="currentColor"
													stroke-width="2.5"
													stroke-linecap="round"
													stroke-linejoin="round"
												>
													<path d="M20 6 9 17l-5-5" />
												</svg>
											</button>
											<span
												:class="[
													'text-sm',
													task.status?.toLowerCase() === 'completed'
														? 'text-ink-subtle line-through'
														: 'text-ink',
												]"
											>
												{{ task.title }}
											</span>
										</div>

										<!-- Right Side: Action Buttons -->
										<div class="flex items-center gap-1">
											<button
												v-if="task.status?.toLowerCase() === 'completed'"
												@click.stop="archiveTask(task)"
												class="flex h-8 w-8 items-center justify-center rounded-pill text-ink-subtle transition-colors hover:bg-surface-hover hover:text-brand"
												title="Archive"
											>
												<svg
													xmlns="http://www.w3.org/2000/svg"
													width="16"
													height="16"
													viewBox="0 0 24 24"
													fill="none"
													stroke="currentColor"
													stroke-width="2"
													stroke-linecap="round"
													stroke-linejoin="round"
												>
													<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
													<line x1="12" y1="11" x2="12" y2="17" />
													<line x1="9" y1="14" x2="15" y2="14" />
												</svg>
											</button>
											<button
												@click.stop="deleteTask(task)"
												class="flex h-8 w-8 items-center justify-center rounded-pill text-ink-subtle transition-colors hover:bg-status-fix-bg hover:text-status-fix-fg"
												title="Delete"
											>
												<svg
													xmlns="http://www.w3.org/2000/svg"
													width="16"
													height="16"
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

						<!-- Task Detail Panel -->
						<Transition name="slide">
							<div v-if="selectedTask" class="right-0 top-0 w-1/2 rounded-card border border-line bg-surface px-5 py-3.5 shadow-tmgr-sm">
								<TaskDetails
									:key="selectedTask?.id"
									:task="selectedTask"
									@update="handleTaskUpdate"
									@reload="reload"
									@close="selectedTask = null"
								/>
							</div>
						</Transition>
					</div>
				</div>
			</div>
		</template>
	</BaseLayout>

	</FeatureGate>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import TextField from '@/components/general/TextField.vue';
import FeatureGate from '@/components/general/FeatureGate.vue';
import { setDocumentTitle } from '@/composable/useDocumentTitle';
import DailyRoutinesPreview from '@/components/previews/DailyRoutinesPreview.vue';
import { SkeletonListItem } from '@/components/ui/skeleton';
import { CalendarCheck } from 'lucide-vue-next';
import {
	createDailyTask,
	getDailyTasks,
	deleteDailyTask,
	completeDailyTask,
	archiveDailyTask,
	getArchivedDailyTasksCount,
	updateDailyTask,
	getDailyTask,
	deleteDailyTaskInstance,
	completeDailyTaskInstance,
	getDailyTasksCount,
	getCompletedDailyTasksCount
} from '@/actions/tmgr/daily-tasks';
import { getTask, Task } from '@/actions/tmgr/tasks';
import TaskDetails from '@/components/TaskDetails.vue';

// Types
interface ExtendedTask extends Task {
	isAnimating?: boolean;
	instance_id?: number;
	description?: string;
	start_date?: string;
	due_date?: string;
}

// State
const tasks = ref<ExtendedTask[]>([]);
const allTasksCount = ref<number>(0);
const archivedTasksCount = ref<number>(0);
const form = ref<Task>({ title: '', status: 'backlog' } as Task);
const selectedTask = ref<ExtendedTask | null>(null);
const isLoading = ref<boolean>(true);

// Computed
const completedCount = ref<number>(0);

const reload = async () => {
	isLoading.value = true;
	try {
		tasks.value = await getDailyTasks();
		archivedTasksCount.value = await getArchivedDailyTasksCount();
		allTasksCount.value = await getDailyTasksCount();
		completedCount.value = await getCompletedDailyTasksCount();
	} finally {
		isLoading.value = false;
	}
}

// Lifecycle
onMounted(async () => {
	setDocumentTitle('Daily Routines');
	await reload();
});

// Task Management
async function selectTask(task: ExtendedTask) {
	selectedTask.value = await getDailyTask(task.id);
	selectedTask.value.instance_id = task.instance_id;
}

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

async function handleTaskUpdate(updatedTask: ExtendedTask) {
	try {
		console.log(updatedTask);
		const result = await updateDailyTask(updatedTask.id, updatedTask);
		const index = tasks.value.findIndex((t) => t.id === result.id);
		if (index !== -1) {
			tasks.value[index] = result;
		}
		selectedTask.value = null;
	} catch (error) {
		console.error('Failed to update task:', error);
	}
}

async function completeTask(task: ExtendedTask) {
	try {
		if (task.instance_id) {
			await completeDailyTaskInstance(task.id, task.instance_id);
			tasks.value = tasks.value.filter((t) => t.instance_id !== task.instance_id);
			if (selectedTask.value?.instance_id === task.instance_id) {
				selectedTask.value = null;
			}
		} else {
			const result = await completeDailyTask(task.id);
			const taskIndex = tasks.value.findIndex((t) => t.id === task.id);
			if (taskIndex !== -1) {
				const statusName = result.status?.name || 'Unknown';
				const updatedTask = {
					...result,
					status: statusName,
					instance_id: null,
					is_instance: false,
					isAnimating: false
				};
				tasks.value[taskIndex] = updatedTask;
			}
		}
		completedCount.value = await getCompletedDailyTasksCount();
	} catch (error) {
		console.error('Failed to complete task:', error);
	}
}

async function deleteTask(task: ExtendedTask) {
	try {
		if (task?.instance_id) {
			await deleteDailyTaskInstance(task.id, task.instance_id);

			tasks.value = tasks.value.filter((t) => t.instance_id !== task.instance_id);
			if (selectedTask.value?.instance_id === task.instance_id) {
				selectedTask.value = null;
			}
		} else {
			await deleteDailyTask(task.id);

			tasks.value = tasks.value.filter((t) => t.id !== task.id);
			if (selectedTask.value?.id === task.id) {
				selectedTask.value = null;
			}
		}
	} catch (error) {
		console.error('Failed to delete task:', error);
	}
}

async function archiveTask(task: ExtendedTask) {
	try {
		await archiveDailyTask(task.id);
		tasks.value = tasks.value.filter((t) => t.id !== task.id);
		if (selectedTask.value?.id === task.id) {
			selectedTask.value = null;
		}
		archivedTasksCount.value = await getArchivedDailyTasksCount();
		completedCount.value = await getCompletedDailyTasksCount();
	} catch (error) {
		console.error('Failed to archive task:', error);
	}
}
</script>

<style>
/* Task list animations */
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
	width: 100%;
}

/* Slide panel animations */
.slide-enter-active,
.slide-leave-active {
	transition: all 0.3s ease-in-out;
}

.slide-enter-from,
.slide-leave-to {
	transform: translateX(100%);
	opacity: 0;
}

/* Task item hover effects */
.task-item {
	transition: all 0.2s ease;
}

.task-item:hover {
	transform: translateX(4px);
}

/* Checkbox animation */
.task-item button svg {
	transition: all 0.2s ease;
}

.task-item button:hover svg {
	transform: scale(1.2);
}
</style>
