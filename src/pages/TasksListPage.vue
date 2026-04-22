<script setup lang="ts">
	import TasksListComponent from '@/components/tasks/TasksListComponent.vue';
	import Confetti from '@/components/Confetti.vue';
	import { getTasks, getTasksByStatus, Task, PaginationMeta } from '@/actions/tmgr/tasks';
	import { getCategories } from '@/actions/tmgr/categories';
	import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import {
		SquareDashedMousePointerIcon,
		SlidersHorizontalIcon,
	} from 'lucide-vue-next';
	import {
		Dialog,
		DialogContent,
		DialogHeader,
		DialogTitle,
		DialogTrigger,
		DialogFooter,
	} from '@/components/ui/dialog';
	import CategoriesCombobox from '@/components/CategoriesCombobox.vue';
	import { Button } from '@/components/ui/button';
	import store from '@/store';
	import { Input } from '@/components/ui/input';
	import { formatTime } from '@/utils/timeUtils.js';
	import EmptyState from '@/components/EmptyState.vue';
	import { Skeleton } from '@/components/ui/skeleton';
	import { getWorkspaceMembers } from '@/actions/tmgr/workspaces';
	import { getUser } from '@/actions/tmgr/user';
	import WorkspaceUsers from '@/components/general/WorkspaceUsers.vue';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { usePusher } from '@/composable/usePusher';

	const route = useRoute();
	const router = useRouter();
	const selectableTasks = ref(false);
	const errorLoading = ref(false);
	const searchText = ref(null);
	const searchTimeout = ref(null);
	const isLoading = ref(true);
	const h1 = {
		CurrentTasksList: 'Current tasks',
		HiddenTasksList: 'Hidden tasks',
		ArchiveTasksList: 'Archive',
		ArchiveTasksListWithWorkspace: 'Archive',
		WorkspaceTasksList: 'Tasks',
		FallbackTasksList: 'Current tasks',
	};
	const tasks = ref<Task[]>([]);
	const isLoadingActions = ref({});
	const hasAbilityToShowConfetti = ref(false);
	const categories = ref([]);
	const selectedCategory = ref(null);
	const showCategorySelect = ref(false);
	const workspaceUsers = ref<Array<{ id: number; name: string }>>([]);
	const workspaceId = ref<number>(0);
	const newCommentTaskIds = ref<Set<number>>(new Set());
	const pusherSubscriptionId = ref<string>('');
	
	const { subscribeToWorkspace, unsubscribeHandlerFromWorkspace } = usePusher();
	const pagination = ref<PaginationMeta>({
		current_page: Number(route.query.page) || 1,
		per_page: Number(route.query.per_page) || 10,
		total: 0,
		last_page: 1,
		from: 0,
		to: 0
	});

	const status = computed(() => route.meta.status);
	const totalSeconds = computed(() => 
		pagination.value?.total_seconds || tasks.value.reduce((summary, task) => task.common_time + summary, 0)
	);
	
	const summaryTime = computed(() => formatTime(totalSeconds.value));
	
	const timeStats = computed(() => {
		const seconds = totalSeconds.value;
		const hours = seconds / 3600;
		const days = hours / 24;
		const workingDays = hours / 8;
		const workingMonths = hours / 160;
		const workingYears = hours / 2000;
		
		return {
			hours: hours.toFixed(1),
			totalDays: days.toFixed(1),
			workingDays: workingDays.toFixed(1),
			workingMonths: workingMonths.toFixed(2),
			workingYears: workingYears.toFixed(2),
		};
	});

	function getApproximatelyTime(task: Task) {
		if ((task as any).approximately_time) {
			return parseInt(String((task as any).approximately_time), 10);
		}
		const setting = (task as any).settings?.find((s: any) => s.key === 'approximately_time');
		if (setting) {
			return parseInt(String(setting.value || setting.pivot?.value), 10);
		}
		return 0;
	}

	const totalOvertime = computed(() => {
		let overtime = 0;
		for (const task of tasks.value) {
			const approximatelyTime = getApproximatelyTime(task);
			if (approximatelyTime > 0) {
				const currentTime = task.common_time || 0;
				if (currentTime > approximatelyTime) {
					overtime += currentTime - approximatelyTime;
				}
			}
		}
		return overtime;
	});

	const formattedTotalOvertime = computed(() => {
		if (totalOvertime.value <= 0) return null;
		const hours = Math.floor(totalOvertime.value / 3600);
		const minutes = Math.floor((totalOvertime.value % 3600) / 60);
		const parts = [];
		if (hours > 0) {
			parts.push(`${hours}h`);
		}
		if (minutes > 0) {
			parts.push(`${minutes}m`);
		}
		return parts.length > 0 ? parts.join(' ') : null;
	});

	function handleKeyDown(event: KeyboardEvent) {
		if (event.key === 'Escape' && selectableTasks.value) {
			selectableTasks.value = false;
		}
	}

	onMounted(async () => {
		try {
			setDocumentTitle(h1[route.name] || 'Task List');
			
			categories.value = await getCategories();

			const user = await getUser();
			const workspaceSetting = user.settings?.find(
				(setting) => setting.key === 'current_workspace',
			);
			if (workspaceSetting) {
				workspaceId.value = +workspaceSetting.value;
				workspaceUsers.value = await getWorkspaceMembers(workspaceId.value);
				
				pusherSubscriptionId.value = subscribeToWorkspace(workspaceId.value, {
					onTaskUpdated: (task, action) => {
						if (action === 'created') {
							const matchesStatus = !status.value || task.status === status.value;
							const exists = tasks.value.some(t => t.id === task.id);
							if (matchesStatus && !exists) {
								tasks.value.unshift(task);
								pagination.value.total++;
							}
						} else if (action === 'updated') {
							const index = tasks.value.findIndex(t => t.id === task.id);
							if (index !== -1) {
								tasks.value.splice(index, 1, task);
							}
						} else if (action === 'deleted') {
							tasks.value = tasks.value.filter(t => t.id !== task.id);
							pagination.value.total = Math.max(0, pagination.value.total - 1);
						}
					},
					onCommentAdded: (comment) => {
						const taskExists = tasks.value.some(t => t.id === comment.task_id);
						if (taskExists) {
							newCommentTaskIds.value.add(comment.task_id);
						}
					}
				});
			}

			await loadTasks();
			setLoadingActions(tasks.value);

			// Check if we're on the root path and have a current workspace
			if (route.path === '/' && store.state.workspaces?.length) {
				// Get current workspace ID
				const currentWorkspaceId = store.state.user?.settings?.find(
					setting => setting.key === 'current_workspace'
				)?.value;
				
				// Find the workspace by ID
				const currentWorkspace = store.state.workspaces.find(
					workspace => Number(workspace.id) === Number(currentWorkspaceId)
				);
				
				if (currentWorkspace?.code) {
					// Set a page title that includes workspace name and task count
					const baseTitle = `${currentWorkspace.name} Tasks`;
					const titleWithCount = pagination.value.total !== undefined 
						? `${baseTitle} (${pagination.value.total})` 
						: baseTitle;
					setDocumentTitle(baseTitle);
					store.commit('setMetaTitle', titleWithCount);
				} else {
					// Update metaTitle with task count for regular routes
					const baseTitle = h1[route.name] || 'Task List';
					const titleWithCount = pagination.value.total !== undefined 
						? `${baseTitle} (${pagination.value.total})` 
						: baseTitle;
					store.commit('setMetaTitle', titleWithCount);
				}
			}

			window.addEventListener('keydown', handleKeyDown);
		} catch (e) {
			console.error(e);
		}
	});

	onUnmounted(() => {
		window.removeEventListener('keydown', handleKeyDown);
		if (workspaceId.value && pusherSubscriptionId.value) {
			unsubscribeHandlerFromWorkspace(workspaceId.value, pusherSubscriptionId.value);
		}
	});

	watch(searchText, () => {
		clearTimeout(searchTimeout.value);
		searchTimeout.value = setTimeout(loadTasks, 500);
	});

	watch(selectedCategory, loadTasks);
	watch(() => store.state.reloadTasksKey, loadTasks);
	watch(() => store.state.updatedTaskKey, () => {
		const updatedTask = store.state.updatedTaskData;
		if (updatedTask) {
			updateSingleTaskInList(updatedTask);
		}
	});
	watch(() => route.name, (newName) => {
		if (newName) {
			setDocumentTitle(h1[newName] || 'Task List');
		}
	});
	watch(() => pagination.value.total, () => {
		if (pagination.value.total !== undefined) {
			const baseTitle = h1[route.name] || 'Task List';
			store.commit('setMetaTitle', `${baseTitle} (${pagination.value.total})`);
		}
	});

	function setLoadingActions(tasks) {
		tasks.forEach((task) => {
			isLoadingActions.value[`hide-${task.id}`] = false;
			isLoadingActions.value[`done-${task.id}`] = false;
			isLoadingActions.value[`start-${task.id}`] = false;
			isLoadingActions.value[`stop-${task.id}`] = false;
			isLoadingActions.value[`activate-${task.id}`] = false;
			isLoadingActions.value[`delete-${task.id}`] = false;
		});
	}

	async function reloadTasks() {
		hasAbilityToShowConfetti.value = true;
		await loadTasks();
	}

	async function loadTasks() {
		try {
			isLoading.value = true;
			clearTimeout(searchTimeout.value);

			const params = {
				params: {
					search: searchText.value,
					project_category_id: selectedCategory.value === -1 ? null : selectedCategory.value,
				},
				page: pagination.value.current_page,
				per_page: pagination.value.per_page
			};

			let response;
			if (status.value) {
				response = await getTasksByStatus(status.value, params);
			} else {
				response = await getTasks(params);
			}

			tasks.value = response.data;
			pagination.value = response.meta;
			setLoadingActions(tasks.value);
			
			const baseTitle = h1[route.name] || 'Task List';
			store.commit('setMetaTitle', `${baseTitle} (${pagination.value.total})`);
		} catch (e) {
			console.error(e);
			errorLoading.value = true;
		} finally {
			isLoading.value = false;
		}
	}

	function handlePageChange(page: number) {
		pagination.value.current_page = page;
		updateRouteQuery();
		loadTasks();
	}

	function handlePerPageChange(perPage: number) {
		pagination.value.per_page = perPage;
		pagination.value.current_page = 1; // Reset to first page when changing items per page
		updateRouteQuery();
		loadTasks();
	}

	function updateRouteQuery() {
		router.push({
			query: {
				...route.query,
				page: pagination.value.current_page,
				per_page: pagination.value.per_page
			}
		});
	}

	function resetFilters() {
		selectedCategory.value = null;
	}

	function updateSingleTaskInList(updatedTask: Task) {
		const taskIndex = tasks.value.findIndex(t => t.id === updatedTask.id);
		
		if (taskIndex !== -1) {
			const shouldStayInList = !status.value || updatedTask.status === status.value;
			
			if (shouldStayInList) {
				tasks.value.splice(taskIndex, 1, updatedTask);
			} else {
				tasks.value.splice(taskIndex, 1);
				pagination.value.total = Math.max(0, pagination.value.total - 1);
			}
		} else {
			const shouldBeInList = !status.value || updatedTask.status === status.value;
			if (shouldBeInList) {
				tasks.value.unshift(updatedTask);
				pagination.value.total++;
			}
		}
	}
</script>

<template>
	<BaseLayout>
		<template #action>
			<div class="flex flex-col gap-2 px-2">
				<div class="w-full py-2">
					<WorkspaceUsers
						:users="workspaceUsers"
						:workspace-id="workspaceId"
					/>
				</div>
				<div class="flex flex-col gap-3">
					<transition name="fade">
						<div
							v-if="summaryTime && status === 'done'"
							class="grid w-full grid-cols-2 gap-3 rounded-card border border-line bg-surface px-4 py-3 sm:grid-cols-4"
						>
							<div class="flex flex-col items-center justify-center text-center">
								<div class="text-2xs font-bold uppercase tracking-wide text-ink-subtle">Total Time</div>
								<div class="text-xl font-semibold tabular-nums text-ink">{{ summaryTime }}</div>
							</div>
							<div class="flex flex-col items-center justify-center text-center">
								<div class="text-2xs font-bold uppercase tracking-wide text-ink-subtle">Working Days</div>
								<div class="text-xl font-semibold text-status-done-fg">{{ timeStats.workingDays }}</div>
								<div class="text-2xs text-ink-faint">(8h/day)</div>
							</div>
							<div class="flex flex-col items-center justify-center text-center">
								<div class="text-2xs font-bold uppercase tracking-wide text-ink-subtle">Working Months</div>
								<div class="text-xl font-semibold text-status-testing-fg">{{ timeStats.workingMonths }}</div>
								<div class="text-2xs text-ink-faint">(160h/month)</div>
							</div>
							<div class="flex flex-col items-center justify-center text-center">
								<div class="text-2xs font-bold uppercase tracking-wide text-ink-subtle">Working Years</div>
								<div class="text-xl font-semibold text-status-progress-fg">{{ timeStats.workingYears }}</div>
								<div class="text-2xs text-ink-faint">(2000h/year)</div>
							</div>
							<div v-if="formattedTotalOvertime" class="col-span-2 flex flex-col items-center justify-center text-center sm:col-span-4">
								<div class="text-2xs font-bold uppercase tracking-wide text-ink-subtle">Overtime</div>
								<div class="text-xl font-semibold text-status-fix-fg">+{{ formattedTotalOvertime }}</div>
							</div>
						</div>
						<div
							v-else-if="summaryTime"
							class="w-full shrink-0 text-center text-2xl font-semibold tabular-nums text-ink sm:text-3xl"
						>
							{{ summaryTime }}
							<span
								v-if="formattedTotalOvertime"
								class="ml-2 text-status-fix-fg"
								title="Total Overtime"
							>
								+{{ formattedTotalOvertime }}
							</span>
						</div>
					</transition>

					<div class="flex w-full items-center justify-end gap-2">
					<div class="relative flex-1">
						<input
							v-model="searchText"
							placeholder="search task"
							type="search"
							class="h-9 w-full rounded-pill border border-line bg-surface pl-4 pr-3 text-sm text-ink placeholder:text-ink-subtle outline-none focus:border-line-strong"
						/>
					</div>

					<Dialog>
						<DialogTrigger as-child>
							<button
								@click="showCategorySelect = !showCategorySelect"
								type="button"
								title="filters"
								class="flex h-9 w-9 shrink-0 items-center justify-center rounded-pill border bg-surface text-ink-subtle transition-colors hover:text-ink"
								:class="[
									selectedCategory && selectedCategory !== -1
										? 'border-brand text-brand'
										: 'border-line',
								]"
							>
								<SlidersHorizontalIcon class="h-4 w-4" />
							</button>
						</DialogTrigger>

						<DialogContent
							class="rounded-card border border-line bg-surface text-ink sm:max-w-[425px]"
						>
							<DialogHeader>
								<DialogTitle>Filters</DialogTitle>
							</DialogHeader>

							<CategoriesCombobox
								:categories="categories"
								v-model="selectedCategory"
								class="!w-full"
							/>
							<DialogFooter>
								<Button variant="outline" @click="resetFilters"> reset </Button>
							</DialogFooter>
						</DialogContent>
					</Dialog>

					<button
						@click="selectableTasks = !selectableTasks"
						type="button"
						title="Tasks selection mode"
						class="flex h-9 w-9 shrink-0 items-center justify-center rounded-pill border bg-surface text-ink-subtle transition-colors hover:text-ink"
						:class="[
							selectableTasks ? 'border-brand text-brand' : 'border-line',
						]"
					>
						<SquareDashedMousePointerIcon class="h-4 w-4" />
					</button>
					</div>
				</div>
			</div>
		</template>

		<template #body>
			<div class="mt-4 min-h-96">
				<tasks-list-component
					v-if="tasks && tasks.length > 0 && !isLoading"
					:tasks="tasks"
					:status="status"
					:is-loading-actions="isLoadingActions"
					:has-selectable="selectableTasks"
					:pagination="pagination"
					@reload-tasks="reloadTasks"
					@page-change="handlePageChange"
					@per-page-change="handlePerPageChange"
					ref="tasksListComponent"
				/>

				<div v-else-if="errorLoading" class="text-center text-xl">
					Something went wrong...
				</div>

				<div v-else-if="!isLoading" class="">
					<EmptyState />

					<confetti v-if="hasAbilityToShowConfetti" />
				</div>

				<div v-if="isLoading" class="mt-6 space-y-2 px-2">
					<Skeleton class="h-28 w-full" />
					<Skeleton class="h-28 w-full" />
					<Skeleton class="h-28 w-full" />
				</div>
			</div>
		</template>
	</BaseLayout>
</template>
