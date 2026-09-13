<script setup lang="ts">
	import { getCategories } from '@/actions/tmgr/categories';
	import {
		getTasks,
		getTasksByStatus,
		PaginationMeta,
		Task,
	} from '@/actions/tmgr/tasks';
	import { getUser } from '@/actions/tmgr/user';
	import {
		getWorkspaceMembers,
		getWorkspaceStatuses,
	} from '@/actions/tmgr/workspaces';
	import CategoriesCombobox from '@/components/CategoriesCombobox.vue';
	import Confetti from '@/components/Confetti.vue';
	import EmptyState from '@/components/EmptyState.vue';
	import WorkspaceUsers from '@/components/general/WorkspaceUsers.vue';
	import TasksListComponent from '@/components/tasks/TasksListComponent.vue';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogFooter,
		DialogHeader,
		DialogTitle,
		DialogTrigger,
	} from '@/components/ui/dialog';
	import { Skeleton } from '@/components/ui/skeleton';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { usePusher } from '@/composable/usePusher';
	import store from '@/store';
	import {
		buildArchivedStatusSets,
		isArchivedTask as isArchivedTaskBySets,
	} from '@/utils/archivedTasks';
	import {
		totalOvertimeSeconds,
		type EstimatedTask,
		type OvertimePagination,
	} from '@/utils/overtime';
	import { createRequestSequence } from '@/utils/requestSequence';
	import { readTaskListQuery } from '@/utils/taskListQuery';
	import { removeTaskFromList, upsertTaskInList } from '@/utils/taskPatch';
	import { formatTime } from '@/utils/timeUtils.js';
	import {
		ArrowDownWideNarrowIcon,
		ArrowUpWideNarrowIcon,
		SlidersHorizontalIcon,
		SquareDashedMousePointerIcon,
	} from 'lucide-vue-next';
	import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
	import { useRoute, useRouter } from 'vue-router';

	const route = useRoute();
	const router = useRouter();
	const selectableTasks = ref(false);
	const errorLoading = ref(false);
	const searchText = ref<string | null>(null);
	const searchTimeout = ref<ReturnType<typeof setTimeout> | null>(null);
	const isLoading = ref(true);
	const hasLoadedTasks = ref(false);
	const requests = createRequestSequence();
	let disposed = false;
	let initialized = false;
	let initializing = false;
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
	const archivedStatusIds = ref<Set<number>>(new Set());
	const archivedStatusNames = ref<Set<string>>(new Set());

	// Date-field sorting for status lists (archive/hidden/done). Mirrors the API's
	// order[column]/order[direction] contract; default matches the backend (updated_at desc).
	const SORT_FIELDS = [
		{ value: 'updated_at', label: 'Updated' },
		{ value: 'created_at', label: 'Created' },
		{ value: 'expired_at', label: 'Deadline' },
		{ value: 'scheduled_date', label: 'Scheduled' },
	] as const;
	const initialQuery = readTaskListQuery(route.query);
	const sortColumn = ref(initialQuery.sort);
	const sortDirection = ref<'asc' | 'desc'>(initialQuery.direction);

	const { subscribeToWorkspace, unsubscribeHandlerFromWorkspace } = usePusher();
	const pagination = ref<PaginationMeta>({
		current_page: initialQuery.page,
		per_page: initialQuery.perPage,
		total: 0,
		last_page: 1,
		from: 0,
		to: 0,
	});

	const status = computed(() => route.meta.status);
	const isActiveList = computed(() => !status.value);

	function isArchivedTask(task: Task) {
		return isArchivedTaskBySets(task, {
			ids: archivedStatusIds.value,
			names: archivedStatusNames.value,
		});
	}

	// Status, archived-state and selectedCategory are all fields we can
	// reliably compare client-side, so a mismatch on any of them is trusted
	// in BOTH directions: it blocks inserting a not-yet-visible task, and it
	// evicts an already-visible one whose data just changed to no longer
	// match. searchText is excluded on purpose - the server's search
	// semantics (title only? description too? fuzzy?) aren't reproducible
	// client-side, so it can't feed a reliable-in-both-directions check;
	// see canInsertTask for how it's actually used.
	function matchesKnownFilters(task: Task): boolean {
		if (status.value && task.status !== status.value) return false;
		if (isActiveList.value && isArchivedTask(task)) return false;
		if (
			selectedCategory.value != null &&
			selectedCategory.value !== -1 &&
			task.project_category_id !== selectedCategory.value
		) {
			return false;
		}
		return true;
	}

	// Whether a realtime event can safely INSERT a not-yet-visible task (or,
	// for a delete of an off-page task, whether it was safely countable in
	// pagination.value.total to begin with). Unlike matchesKnownFilters, an
	// active search says no here - we can't verify a not-yet-visible task
	// matches the search, so the safe default is to skip rather than guess.
	// This must NOT be used to decide whether to EVICT an already-visible
	// task: an active search we can't verify means "leave it alone", not
	// "remove it" - callers with a task already in tasks.value should evict
	// only on a matchesKnownFilters mismatch, never merely because a search
	// is active.
	function canInsertTask(task: Task): boolean {
		return !searchText.value && matchesKnownFilters(task);
	}

	const totalSeconds = computed(
		() =>
			pagination.value?.total_seconds ||
			tasks.value.reduce((summary, task) => task.common_time + summary, 0),
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

	// Overtime over every matching task: the server sums it across all pages
	// (`meta.total_overtime_seconds`, TM-130); the page sum is only a fallback.
	const totalOvertime = computed(() =>
		totalOvertimeSeconds(
			pagination.value as OvertimePagination | null,
			tasks.value as EstimatedTask[],
		),
	);

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

	onMounted(initializeList);

	async function initializeList() {
		if (disposed || initializing) return;
		initializing = true;
		isLoading.value = true;
		errorLoading.value = false;
		try {
			setDocumentTitle(h1[route.name] || 'Task List');

			categories.value = await getCategories();
			if (disposed) return;

			try {
				const workspaceStatuses = await getWorkspaceStatuses();
				if (disposed) return;
				const archivedSets = buildArchivedStatusSets(workspaceStatuses);
				archivedStatusIds.value = archivedSets.ids;
				archivedStatusNames.value = archivedSets.names;
			} catch (statusError) {
				console.error(statusError);
			}

			// Same as the board: the guard has the user by now, so do not ask again (TM-218).
			const user = store.state.user?.id ? store.state.user : await getUser();
			if (disposed) return;
			const workspaceSetting = user.settings?.find(
				(setting) => setting.key === 'current_workspace',
			);
			if (workspaceSetting) {
				workspaceId.value = +workspaceSetting.value;
				workspaceUsers.value = await getWorkspaceMembers(workspaceId.value);
				if (disposed) return;

				pusherSubscriptionId.value = subscribeToWorkspace(workspaceId.value, {
					onTaskUpdated: (task, action) => {
						if (action === 'deleted') {
							removeTaskFromList_(task);
						} else {
							updateSingleTaskInList(task);
						}
					},
					onCommentAdded: (comment) => {
						const taskExists = tasks.value.some(
							(t) => t.id === comment.task_id,
						);
						if (taskExists) {
							newCommentTaskIds.value.add(comment.task_id);
						}
					},
				});
			}

			initialized = true;
			await loadTasks();
			if (disposed) return;
			setLoadingActions(tasks.value);

			// Check if we're on the root path and have a current workspace
			if (route.path === '/' && store.state.workspaces?.length) {
				// Get current workspace ID
				const currentWorkspaceId = store.state.user?.settings?.find(
					(setting) => setting.key === 'current_workspace',
				)?.value;

				// Find the workspace by ID
				const currentWorkspace = store.state.workspaces.find(
					(workspace) => Number(workspace.id) === Number(currentWorkspaceId),
				);

				if (currentWorkspace?.code) {
					// Set a page title that includes workspace name and task count
					const baseTitle = `${currentWorkspace.name} Tasks`;
					const titleWithCount =
						pagination.value.total !== undefined
							? `${baseTitle} (${pagination.value.total})`
							: baseTitle;
					setDocumentTitle(baseTitle);
					store.commit('setMetaTitle', titleWithCount);
				} else {
					// Update metaTitle with task count for regular routes
					const baseTitle = h1[route.name] || 'Task List';
					const titleWithCount =
						pagination.value.total !== undefined
							? `${baseTitle} (${pagination.value.total})`
							: baseTitle;
					store.commit('setMetaTitle', titleWithCount);
				}
			}

			window.addEventListener('keydown', handleKeyDown);
		} catch (e) {
			if (disposed) return;
			console.error(e);
			errorLoading.value = true;
			isLoading.value = false;
		} finally {
			initializing = false;
		}
	}

	function retryList() {
		return initialized ? loadTasks() : initializeList();
	}

	onBeforeUnmount(() => {
		disposed = true;
		requests.dispose();
		if (searchTimeout.value) clearTimeout(searchTimeout.value);
		window.removeEventListener('keydown', handleKeyDown);
		if (workspaceId.value && pusherSubscriptionId.value) {
			unsubscribeHandlerFromWorkspace(
				workspaceId.value,
				pusherSubscriptionId.value,
			);
		}
	});

	watch(searchText, () => {
		requests.begin();
		if (searchTimeout.value) clearTimeout(searchTimeout.value);
		searchTimeout.value = setTimeout(loadTasks, 500);
	});
	watch(
		() => [
			route.query.page,
			route.query.per_page,
			route.query.sort,
			route.query.direction,
		],
		() => {
			const query = readTaskListQuery(route.query);
			pagination.value.current_page = query.page;
			pagination.value.per_page = query.perPage;
			sortColumn.value = query.sort;
			sortDirection.value = query.direction;
			if (initialized) loadTasks();
		},
	);

	watch(selectedCategory, loadTasks);
	watch(() => store.state.reloadTasksKey, loadTasks);
	watch(
		() => store.state.updatedTaskKey,
		() => {
			const updatedTask = store.state.updatedTaskData;
			if (updatedTask) {
				updateSingleTaskInList(updatedTask);
			}
		},
	);
	watch(
		() => store.state.createdTaskKey,
		() => {
			const createdTask = store.state.createdTaskData;
			if (createdTask) {
				updateSingleTaskInList(createdTask);
			}
		},
	);
	watch(
		() => store.state.deletedTaskKey,
		() => {
			const deletedId = store.state.deletedTaskId;
			if (deletedId) {
				removeTaskFromList_(deletedId);
			}
		},
	);
	watch(
		() => route.name,
		(newName) => {
			if (newName) {
				setDocumentTitle(h1[newName] || 'Task List');
			}
		},
	);
	watch(
		() => pagination.value.total,
		() => {
			if (pagination.value.total !== undefined) {
				const baseTitle = h1[route.name] || 'Task List';
				store.commit(
					'setMetaTitle',
					`${baseTitle} (${pagination.value.total})`,
				);
			}
		},
	);

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
		if (disposed || !initialized) return;
		const request = requests.begin();
		try {
			isLoading.value = true;
			errorLoading.value = false;
			if (searchTimeout.value) clearTimeout(searchTimeout.value);

			const params = {
				params: {
					search: searchText.value,
					project_category_id:
						selectedCategory.value === -1 ? null : selectedCategory.value,
					...(status.value
						? {
								'order[column]': sortColumn.value,
								'order[direction]': sortDirection.value,
						  }
						: {}),
				},
				page: pagination.value.current_page,
				per_page: pagination.value.per_page,
			};

			let response;
			if (status.value) {
				response = await getTasksByStatus(status.value, params);
			} else {
				response = await getTasks(params);
			}

			if (!requests.isCurrent(request)) return;
			tasks.value = response.data;
			pagination.value = response.meta;
			hasLoadedTasks.value = true;
			setLoadingActions(tasks.value);

			const baseTitle = h1[route.name] || 'Task List';
			store.commit('setMetaTitle', `${baseTitle} (${pagination.value.total})`);
		} catch (e) {
			if (!requests.isCurrent(request)) return;
			console.error(e);
			errorLoading.value = true;
		} finally {
			if (requests.isCurrent(request)) isLoading.value = false;
		}
	}

	function handlePageChange(page: number) {
		pagination.value.current_page = page;
		updateRouteQuery();
	}

	function handlePerPageChange(perPage: number) {
		pagination.value.per_page = perPage;
		pagination.value.current_page = 1; // Reset to first page when changing items per page
		updateRouteQuery();
	}

	function updateRouteQuery() {
		router.push({
			query: {
				...route.query,
				page: pagination.value.current_page,
				per_page: pagination.value.per_page,
				...(status.value
					? { sort: sortColumn.value, direction: sortDirection.value }
					: {}),
			},
		});
	}

	function changeSort() {
		pagination.value.current_page = 1;
		updateRouteQuery();
	}

	function toggleSortDirection() {
		sortDirection.value = sortDirection.value === 'asc' ? 'desc' : 'asc';
		changeSort();
	}

	function resetFilters() {
		selectedCategory.value = null;
	}

	const hasActiveSearch = computed(
		() =>
			!!searchText.value ||
			(selectedCategory.value !== null && selectedCategory.value !== -1),
	);

	function clearSearch() {
		searchText.value = null;
		resetFilters();
	}

	function openCreateTaskModal() {
		store.commit('setShowCreatingTaskModal');
	}

	function updateSingleTaskInList(updatedTask: Task) {
		const known = tasks.value.some((x) => x.id === updatedTask.id);
		const result = upsertTaskInList(tasks.value as any[], updatedTask as any, {
			accepts: (t) =>
				known ? matchesKnownFilters(t as Task) : canInsertTask(t as Task),
			firstPage: pagination.value.current_page === 1,
		});
		if (result === 'inserted') pagination.value.total++;
		if (result === 'removed')
			pagination.value.total = Math.max(0, pagination.value.total - 1);
	}

	// A deleted task counts against the total when it was on this page OR would have
	// been listed on another page (same rule as inserting one), so the total does not
	// drift on realtime deletes of tasks outside the current page.
	function removeTaskFromList_(deleted: Task | number) {
		const taskId = typeof deleted === 'number' ? deleted : deleted.id;
		const removed = removeTaskFromList(tasks.value as any[], taskId as number);
		const offPageMatch =
			typeof deleted !== 'number' && !removed && canInsertTask(deleted);
		if (removed || offPageMatch) {
			pagination.value.total = Math.max(0, pagination.value.total - 1);
		}
	}
</script>

<template>
	<BaseLayout>
		<template #action>
			<div class="flex flex-col gap-2 px-2">
				<div class="w-full py-2">
					<WorkspaceUsers :users="workspaceUsers" :workspace-id="workspaceId" />
				</div>
				<div class="flex flex-col gap-3">
					<transition name="fade">
						<div
							v-if="summaryTime && status === 'done'"
							class="grid w-full grid-cols-2 gap-3 rounded-card border border-line bg-surface px-4 py-3 sm:grid-cols-4"
						>
							<div
								class="flex flex-col items-center justify-center text-center"
							>
								<div
									class="text-2xs font-bold uppercase tracking-wide text-ink-subtle"
								>
									Total Time
								</div>
								<div class="text-xl font-semibold tabular-nums text-ink">
									{{ summaryTime }}
								</div>
							</div>
							<div
								class="flex flex-col items-center justify-center text-center"
							>
								<div
									class="text-2xs font-bold uppercase tracking-wide text-ink-subtle"
								>
									Working Days
								</div>
								<div class="text-xl font-semibold text-status-done-fg">
									{{ timeStats.workingDays }}
								</div>
								<div class="text-2xs text-ink-faint">(8h/day)</div>
							</div>
							<div
								class="flex flex-col items-center justify-center text-center"
							>
								<div
									class="text-2xs font-bold uppercase tracking-wide text-ink-subtle"
								>
									Working Months
								</div>
								<div class="text-xl font-semibold text-status-testing-fg">
									{{ timeStats.workingMonths }}
								</div>
								<div class="text-2xs text-ink-faint">(160h/month)</div>
							</div>
							<div
								class="flex flex-col items-center justify-center text-center"
							>
								<div
									class="text-2xs font-bold uppercase tracking-wide text-ink-subtle"
								>
									Working Years
								</div>
								<div class="text-xl font-semibold text-status-progress-fg">
									{{ timeStats.workingYears }}
								</div>
								<div class="text-2xs text-ink-faint">(2000h/year)</div>
							</div>
							<div
								v-if="formattedTotalOvertime"
								class="col-span-2 flex flex-col items-center justify-center text-center sm:col-span-4"
							>
								<div
									class="text-2xs font-bold uppercase tracking-wide text-ink-subtle"
								>
									Overtime
								</div>
								<div class="text-xl font-semibold text-status-fix-fg">
									+{{ formattedTotalOvertime }}
								</div>
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
								class="h-9 w-full rounded-pill border border-line bg-surface pl-4 pr-3 text-sm text-ink outline-none placeholder:text-ink-subtle focus:border-line-strong"
							/>
						</div>

						<div v-if="status" class="flex shrink-0 items-center gap-2">
							<select
								v-model="sortColumn"
								@change="changeSort"
								title="Sort by date"
								class="h-9 rounded-pill border border-line bg-surface pl-3 pr-7 text-sm text-ink outline-none focus:border-line-strong"
							>
								<option
									v-for="field in SORT_FIELDS"
									:key="field.value"
									:value="field.value"
								>
									{{ field.label }}
								</option>
							</select>
							<button
								@click="toggleSortDirection"
								type="button"
								:title="
									sortDirection === 'desc' ? 'Newest first' : 'Oldest first'
								"
								class="flex h-9 w-9 shrink-0 items-center justify-center rounded-pill border border-line bg-surface text-ink-subtle transition-colors hover:text-ink"
							>
								<ArrowDownWideNarrowIcon
									v-if="sortDirection === 'desc'"
									class="h-4 w-4"
								/>
								<ArrowUpWideNarrowIcon v-else class="h-4 w-4" />
							</button>
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
									<Button variant="outline" @click="resetFilters">
										reset
									</Button>
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
			<div class="mt-4 min-h-96" :aria-busy="isLoading">
				<div
					class="flex min-h-6 items-center justify-between text-sm text-ink-muted"
					role="status"
				>
					<span v-if="isLoading && hasLoadedTasks">Updating tasks…</span>
					<template v-if="errorLoading">
						<span>Could not load tasks. Please try again.</span>
						<Button
							variant="outline"
							size="sm"
							@click="retryList"
							:disabled="isLoading"
							>Retry</Button
						>
					</template>
				</div>
				<tasks-list-component
					v-if="tasks && tasks.length > 0"
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

				<div v-else-if="!isLoading && !errorLoading" class="">
					<EmptyState
						v-if="hasActiveSearch"
						title="No tasks match your search"
						:action="{ label: 'Clear search', onClick: clearSearch }"
					/>
					<EmptyState
						v-else
						description="Create your first task to get started"
						:action="{ label: '+ Create task', onClick: openCreateTaskModal }"
					/>

					<confetti v-if="hasAbilityToShowConfetti" />
				</div>

				<div
					v-if="isLoading && !hasLoadedTasks"
					class="mt-6 space-y-2 px-2"
					aria-hidden="true"
				>
					<Skeleton class="h-28 w-full" />
					<Skeleton class="h-28 w-full" />
					<Skeleton class="h-28 w-full" />
				</div>
			</div>
		</template>
	</BaseLayout>
</template>
