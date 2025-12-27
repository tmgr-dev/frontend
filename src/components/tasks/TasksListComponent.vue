<template>
	<div class="relative w-full items-center justify-center">
		<Transition name="bounce">
			<div>
				<TasksMultipleActionsModal
					v-if="isShowSelectedTasksCommonTime"
					:is-loading-actions="loadingActionsForMultipleTasks"
					:status="status"
					@close="closeTimeInModal"
					@export="exportSelectedTasks"
					@remove="deleteSelectedTasks"
					@updateStatus="updateStatusForSelectedTasks"
				>
					<p class="text-center text-white">
						<b>Selected tasks: </b>{{ selected.filter(Boolean).length }}<br />
						{{ timeForModal }}
					</p>
				</TasksMultipleActionsModal>
			</div>
		</Transition>

		<div
			:key="hasSelectable"
			v-selectable="
				hasSelectable ? { selectedGetter, selectedSetter, selectingSetter } : {}
			"
			class="relative border pb-2"
			:class="[
				hasSelectable
					? 'rounded-2xl border-dashed border-orange-200 px-2'
					: 'border-transparent',
			]"
		>
			<div
				class="border-black dark:border-gray-400"
				:class="[hasSelectable ? 'selection' : '']"
			/>

			<div
				v-for="(task, i) in tasks"
				:key="i"
				:class="{
					selected: !!selected[i],
					selecting: !!selecting[i],
					'opacity-50 hover:opacity-100': task.deleted_at,
				}"
				:data-task-id="task.id"
				:draggable="false"
				class="selectable relative mt-2 w-full rounded-lg px-2"
			>
				<BounceLoader
					v-if="loadingActionTasksIds.includes(task.id)"
					class="l-0 t-0 absolute z-10"
				/>

				<button
					type="button"
					:class="{
						'border-l-8 border-solid border-green-600': task.start_time,
						'border-b-4 border-b-red-500 dark:border-b-red-400': isTimeExceeded(task),
					}"
					class="w-full rounded-lg shadow-md md:flex"
					@click="$store.commit('setCurrentTaskIdForModal', task.id)"
					@mouseenter="hoveredTaskId = task.id"
					@mouseleave="hoveredTaskId = null"
				>
					<div
						class="w-full space-y-1 rounded-lg bg-white px-4 pb-2 pt-4 transition-colors duration-300 hover:bg-gray-100 dark:bg-gray-900 hover:dark:bg-gray-800"
					>
					<div class="flex items-start gap-2">
						<div 
							v-if="draggable"
							class="task-drag-handle flex-shrink-0 cursor-grab active:cursor-grabbing text-gray-400 hover:text-gray-600 dark:text-gray-500 dark:hover:text-gray-300 pt-0.5"
							:draggable="true"
							@dragstart="onDragStart($event, task)"
							@click.stop
						>
							<span class="material-icons text-base">drag_indicator</span>
						</div>
						<div class="flex-1">
							<CategoryBadge
								v-if="showCategoryBadges"
								class="shrink-0 self-start"
								:category="task.category"
							/>

							<div class="text-left text-sm font-medium lg:text-lg" :title="task.title?.length > 60 ? task.title : ''">
								{{ truncateTitle(task.title) }}
							</div>
						</div>
					</div>

						<div class="flex items-center gap-2">
							<span
								:class="isTimeExceeded(task) ? 'text-red-600 dark:text-red-400' : 'text-green-600 dark:text-green-400'"
								class="material-icons text-xs sm:text-base md:text-xl"
							>
								alarm
							</span>

							<span class="text-xs text-gray-700 dark:text-gray-400 sm:text-base md:text-base">
								{{ getTaskFormattedTime(task) }}
							</span>
							
							<button
								v-if="!task.start_time"
								v-tooltip.top="setTooltipData('Start timer')"
								:disabled="isLoadingActions[`start-${task.id}`]"
								class="flex items-center justify-center rounded bg-green-500/80 p-0.5 text-white hover:bg-green-500 disabled:opacity-50 dark:bg-green-600/70 dark:hover:bg-green-600/90"
								@click.stop="startCountdown(task, `start-${task.id}`)"
							>
								<span v-if="!isLoadingActions[`start-${task.id}`]" class="material-icons text-xs sm:text-sm leading-none">play_arrow</span>
								<Loader v-else is-mini />
							</button>
							<button
								v-else
								v-tooltip.top="setTooltipData('Stop timer')"
								:disabled="isLoadingActions[`stop-${task.id}`]"
								class="flex items-center justify-center rounded bg-red-500/80 p-0.5 text-white hover:bg-red-500 disabled:opacity-50 dark:bg-red-600/70 dark:hover:bg-red-600/90"
								@click.stop="stopCountdown(task, `stop-${task.id}`)"
							>
								<span v-if="!isLoadingActions[`stop-${task.id}`]" class="material-icons text-xs sm:text-sm leading-none">stop</span>
								<Loader v-else is-mini />
							</button>
							
						<Popover v-model:open="assigneePopoverOpen[task.id]">
								<PopoverTrigger as-child>
									<button
										v-show="task.assignees?.length || hoveredTaskId === task.id || assigneePopoverOpen[task.id]"
										class="ml-2 flex h-7 w-7 items-center justify-center rounded hover:bg-gray-200 dark:hover:bg-gray-700 group"
										@click.stop
										:title="task.assignees?.length ? 'Change assignee' : 'Assign someone'"
									>
										<UserPlus 
											v-if="assigneePopoverOpen[task.id] || !task.assignees?.length" 
											class="h-5 w-5 text-gray-500 dark:text-gray-400" 
										/>
										<template v-else>
											<UserPlus class="hidden h-5 w-5 text-gray-500 group-hover:block dark:text-gray-400" />
											<AssigneeUsers
												class="group-hover:hidden"
												:assignees="task.assignees" 
												avatarsClass="h-6 w-6" 
												:show-assignee-controls="false"
											/>
										</template>
									</button>
								</PopoverTrigger>
								<PopoverContent class="z-50 w-52 p-0" align="start" side="bottom" @click.stop>
									<Command>
										<CommandInput placeholder="Search members..." class="h-9" />
										<CommandEmpty>No members found.</CommandEmpty>
										<CommandList>
											<CommandGroup>
												<CommandItem
													v-for="member in workspaceMembers"
													:key="member.id"
													:value="member.id"
													@select="toggleAssignee(task, member.id)"
													class="cursor-pointer"
												>
													<Check
														:class="['mr-2 h-4 w-4', isAssignedTo(task, member.id) ? 'opacity-100' : 'opacity-0']"
													/>
													{{ member.name }}
												</CommandItem>
											</CommandGroup>
										</CommandList>
									</Command>
								</PopoverContent>
							</Popover>
						</div>

						<div class="mt-1 flex items-center gap-x-2 pt-1 text-[9px] text-gray-400/50 dark:text-gray-500/50">
							<span v-if="task.created_at" :title="'Created: ' + formatFullDate(task.created_at)">
								{{ formatRelativeTime(task.created_at) }}
							</span>
							<span v-if="task.updated_at && task.updated_at !== task.created_at" class="flex items-center gap-0.5" :title="'Edited: ' + formatFullDate(task.updated_at)">
								<span class="material-icons" style="font-size: 9px;">edit</span>
								{{ formatRelativeTime(task.updated_at) }}
							</span>
							<span class="ml-auto flex items-center gap-1">
								<span>{{ getStatusName(task) }}</span>
								<span class="inline-block h-2 w-2 rounded-full" :style="{ backgroundColor: getStatusColor(task) }"></span>
							</span>
						</div>
					</div>

					<div class="ml-auto">
						<TaskButtonsInTheList
							:is-loading-actions="isLoadingActions"
							:showed-buttons="getShowButtons(task)"
							:task="task"
							@startCountdown="startCountdown"
							@stopCountdown="stopCountdown"
							@updateStatus="updateStatus"
						/>
					</div>
				</button>
			</div>

			<!-- Add pagination controls -->
			<div v-if="pagination.total > pagination.per_page" class="mt-4 flex items-center justify-between px-4">
				<div class="flex items-center gap-2">
					<span class="text-sm text-gray-600 dark:text-gray-300">
						Showing {{ pagination.from }} to {{ pagination.to }} of {{ pagination.total }} tasks
					</span>
					
					<select 
						v-model="perPage" 
						@change="onPerPageChange"
						class="rounded-md border border-gray-300 bg-white px-2 py-1 text-sm dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
					>
						<option :value="10">10 per page</option>
						<option :value="25">25 per page</option>
						<option :value="50">50 per page</option>
					</select>
				</div>
				
				<div class="flex items-center gap-2">
					<button
						:disabled="pagination.current_page === 1"
						@click="onPageChange(pagination.current_page - 1)"
						class="rounded-md border border-gray-300 bg-white px-3 py-1 text-sm disabled:opacity-50 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
					>
						Previous
					</button>
					
					<span class="text-sm text-gray-600 dark:text-gray-300">
						Page {{ pagination.current_page }} of {{ pagination.last_page }}
					</span>
					
					<button
						:disabled="pagination.current_page === pagination.last_page"
						@click="onPageChange(pagination.current_page + 1)"
						class="rounded-md border border-gray-300 bg-white px-3 py-1 text-sm disabled:opacity-50 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
					>
						Next
					</button>
				</div>
			</div>
		</div>
	</div>

	<Confirm
		v-if="confirm"
		:body="confirm.body"
		:title="confirm.title"
		@onCancel="confirm = undefined; backlogStatusChangeConfirm = null"
		@onOk="confirm.action(); confirm = undefined; backlogStatusChangeConfirm = null"
	/>
</template>

<script lang="ts">
	import downloadFile from '@/utils/downloadFile';
	import { formatRelativeTime } from '@/utils/timeUtils';
	import Loader from '@/components/loaders/Loader.vue';
	import Confirm from '@/components/general/Confirm.vue';
	import TasksListMixin from '@/mixins/TasksListMixin';
	import SetTooltipData from '@/mixins/SetTooltipData';
	import BounceLoader from '@/components/loaders/BounceLoader.vue';
	import TaskActionsInTheListMixin from '@/mixins/TaskActionsInTheListMixin';
	import TaskButtonsInTheList from '@/components/tasks/TaskButtonsInTheList.vue';
	import TasksMultipleActionsModal from '@/components/tasks/TasksMultipleActionsModal.vue';
	import Modal from '@/components/Modal.vue';
	import TaskForm from '@/pages/TaskForm.vue';
	import {
		exportTasks,
		getTask,
		startTaskTimeCounter,
		stopTaskTimeCounter,
		updateTaskStatus,
		updateTaskPartially,
		Task,
		PaginationMeta,
	} from '@/actions/tmgr/tasks';
	import { getStatuses, Status } from '@/actions/tmgr/statuses';
	import { getWorkspaceMembers, WorkspaceMember } from '@/actions/tmgr/workspaces';
	import CategoryBadge from '@/components/general/CategoryBadge.vue';
	import Button from '@/components/general/Button.vue';
	import { PropType } from 'vue';
	import AssigneeUsers from '@/components/general/AssigneeUsers.vue';
	import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
	import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from '@/components/ui/command';
	import { UserPlus, Check } from 'lucide-vue-next';

	export default {
		name: 'TasksListComponent',
		components: {
			Button,
			CategoryBadge,
			TaskForm,
			Modal,
			Confirm,
			BounceLoader,
			Loader,
			TasksMultipleActionsModal,
			TaskButtonsInTheList,
			AssigneeUsers,
			Popover,
			PopoverContent,
			PopoverTrigger,
			Command,
			CommandEmpty,
			CommandGroup,
			CommandInput,
			CommandItem,
			CommandList,
			UserPlus,
			Check,
		},
		emits: ['reload-tasks', 'page-change', 'per-page-change'],
		props: {
			tasks: {
				type: Array as PropType<Task[]>,
				required: false,
				default: () => [],
			},
			status: {
				required: false,
				type: String,
				default: null,
			},
			useTaskStatusForButtons: {
				required: false,
				type: Boolean,
				default: false,
			},
			showCategoryBadges: {
				required: false,
				type: Boolean,
				default: true,
			},
			isLoadingActions: {
				required: true,
				type: Object,
			},
			draggable: {
				type: Boolean,
				required: false,
				default: false,
			},
			hasSelectable: {
				type: Boolean,
				required: false,
				default: false,
			},
			loadingActionTasksIds: {
				type: Array,
				required: false,
				default: () => [],
			},
			pagination: {
				type: Object as PropType<PaginationMeta>,
				required: true,
				default: () => ({
					current_page: 1,
					per_page: 10,
					total: 0,
					last_page: 1,
					from: 0,
					to: 0
				})
			}
		},
		watch: {
			hasSelectable(v) {
				if (!v) {
					this.selected = [];
					this.isShowSelectedTasksCommonTime = false;
				}
			},
			'pagination.per_page': {
				immediate: true,
				handler(value) {
					this.perPage = value;
				}
			}
		},
		mixins: [TasksListMixin, TaskActionsInTheListMixin, SetTooltipData],
		data: () => ({
			showTaskForm: false,
			modalTaskId: null,
			confirm: null,
			isShowSelectedTasksCommonTime: false,
			selected: [],
			selecting: [],
			showTimeInModal: false,
			timeForModal: null,
			loadingActionsForMultipleTasks: [],
			perPage: 10,
			statuses: [] as Status[],
			backlogStatusChangeConfirm: null as { task: Task; dotId: string | null } | null,
			hoveredTaskId: null as number | null,
			assigneePopoverOpen: {} as Record<number, boolean>,
			workspaceMembers: [] as WorkspaceMember[],
		}),
		async created() {
			try {
				this.statuses = await getStatuses();
			} catch (e) {
				console.error('Failed to load statuses:', e);
			}
		},
		watch: {
			assigneePopoverOpen: {
				deep: true,
				handler(newVal) {
					if (Object.values(newVal).some(v => v)) {
						this.loadWorkspaceMembers();
					}
				}
			}
		},
		methods: {
			truncateTitle(title: string) {
				if (!title) return '';
				return title.length > 60 ? title.substring(0, 60) + '...' : title;
			},
			async loadWorkspaceMembers() {
				if (this.workspaceMembers.length > 0) return;
				try {
					const workspaceId = this.$store.state.user?.settings?.find((s: any) => s.key === 'current_workspace')?.value;
					if (workspaceId) {
						this.workspaceMembers = await getWorkspaceMembers(workspaceId);
					}
				} catch (e) {
					console.error('Failed to load workspace members:', e);
				}
			},
			getStatusColor(task: Task) {
				const status = this.statuses.find((s: any) => s.id === task.status_id);
				return status?.color || '#6b7280';
			},
			getStatusName(task: Task) {
				const status = this.statuses.find((s: any) => s.id === task.status_id);
				return status?.name || '';
			},
			isAssignedTo(task: Task, memberId: number) {
				return task.assignees?.some((a: any) => a.id === memberId) || false;
			},
			async toggleAssignee(task: Task, memberId: number) {
				const currentAssigneeIds = task.assignees?.map((a: any) => a.id) || [];
				let newAssigneeIds: number[];
				
				if (currentAssigneeIds.includes(memberId)) {
					newAssigneeIds = currentAssigneeIds.filter((id: number) => id !== memberId);
				} else {
					newAssigneeIds = [...currentAssigneeIds, memberId];
				}
				
				try {
					await updateTaskPartially(task.id!, { assignees: newAssigneeIds } as any);
					const member = this.workspaceMembers.find((m: WorkspaceMember) => m.id === memberId);
					if (currentAssigneeIds.includes(memberId)) {
						task.assignees = task.assignees?.filter((a: any) => a.id !== memberId);
					} else if (member) {
						if (!task.assignees) task.assignees = [];
						(task.assignees as any[]).push(member);
					}
				} catch (e) {
					console.error('Failed to update assignees:', e);
				}
			},
			closeTaskModal() {
				this.$store.commit('closeTaskModal');
			},
			async stopCountdown(task, dotId) {
				this.isLoadingActions[dotId] = true;
				await stopTaskTimeCounter(task.id);
				await this.loadTasks();
				this.isLoadingActions[dotId] = false;
			},
			async startCountdown(task: Task, dotId: string | null) {
				if (!task.id) return;
				
				if (dotId) {
					this.isLoadingActions[dotId] = true;
				}
				await startTaskTimeCounter(task.id);
				await this.loadTasks();
				if (dotId) {
					this.isLoadingActions[dotId] = false;
				}

				const taskStatus = this.statuses.find(s => s.id === task.status_id);
				
				if (taskStatus && taskStatus.type === 'default') {
					this.backlogStatusChangeConfirm = { task, dotId };
					this.showBacklogStatusChangeConfirm();
				}
			},
			showBacklogStatusChangeConfirm() {
				if (!this.backlogStatusChangeConfirm) return;
				
				const { task, dotId } = this.backlogStatusChangeConfirm;
				const activeStatus = this.statuses.find(s => s.type === 'active');
				
				if (!activeStatus || !task.id) {
					console.error('No active status found or task has no id');
					this.backlogStatusChangeConfirm = null;
					return;
				}
				
				this.showConfirm(
					'Task in Backlog',
					`Task "${task.title}" is in backlog. Switch to "${activeStatus.name}" status?`,
					async () => {
						if (!this.backlogStatusChangeConfirm || !task.id) return;
						
						const { task: confirmTask, dotId: confirmDotId } = this.backlogStatusChangeConfirm;
						if (confirmDotId) {
							this.isLoadingActions[confirmDotId] = true;
						}
						
						try {
							await updateTaskStatus(confirmTask.id!, activeStatus.id);
							this.$store.commit('incrementReloadTasksKey');
							await this.loadTasks();
						} catch (e) {
							console.error('Failed to change status:', e);
						} finally {
							if (confirmDotId) {
								this.isLoadingActions[confirmDotId] = false;
							}
							this.backlogStatusChangeConfirm = null;
						}
					}
				);
			},
			onDragStart(event, task) {
				event.dataTransfer.setData('task-id', task.id);
			},
			loadTasks() {
				this.$emit('reload-tasks');
			},
			getShowButtons(task) {
				return {
					start: task.start_time,
					stop: !task.start_time,
					deleteTask: this.status === 'hidden' || this.status === 'done',
				};
			},
			getApproximatelyTime(task: Task) {
				if (task.approximately_time) {
					return parseInt(String(task.approximately_time), 10);
				}
				const setting = task.settings?.find((s: any) => s.key === 'approximately_time');
				if (setting) {
					return parseInt(String(setting.value || setting.pivot?.value), 10);
				}
				return 0;
			},
			isTimeExceeded(task: Task) {
				const approximatelyTime = this.getApproximatelyTime(task);
				if (!approximatelyTime || approximatelyTime <= 0) {
					return false;
				}
				const currentTime = task.common_time || 0;
				return currentTime > approximatelyTime;
			},
			async updateStatus(task, status, dotId = null, loadTasks = true) {
				try {
					this.setLoadingAction(dotId);
					await updateTaskStatus(task.id, status);

					if (loadTasks) {
						this.loadTasks();
					}
				} catch (e) {
					console.error(e);
				} finally {
					this.setLoadingAction(dotId, false);
				}
			},
			async updateStatusForSelectedTasks(status) {
				this.loadingActionsForMultipleTasks.push(status);
				for (let i = 0; i < this.tasks.length; ++i) {
					if (!this.selected[i]) {
						continue;
					}
					await this.updateStatus(this.tasks[i], status, null, false);
				}
				await this.loadTasks();
				this.loadingActionsForMultipleTasks =
					this.loadingActionsForMultipleTasks.filter((s) => s !== status);
				this.resetSelectedTasks();
			},
			showConfirm(title, body, action) {
				this.confirm = { title, body, action };
			},
			formatRelativeTime(dateString: string) {
				return formatRelativeTime(dateString);
			},
			formatFullDate(dateString: string) {
				if (!dateString) return '';
				const date = new Date(dateString);
				return date.toLocaleString('en-US', {
					year: 'numeric',
					month: 'long',
					day: 'numeric',
					hour: '2-digit',
					minute: '2-digit',
					second: '2-digit',
					hour12: false
				});
			},
			deleteSelectedTasks() {
				const deleteMultipleTasks = async () => {
					try {
						this.loadingActionsForMultipleTasks.push('delete');
						for (let i = 0; i < this.tasks.length; ++i) {
							if (!this.selected[i]) {
								continue;
							}
							const data = await getTask(this.tasks[i].id);

							this.tasks[i].deleted_at = data.deleted_at;
						}
					} catch (e) {
						console.error(e);
					} finally {
						this.confirm = null;
						this.loadingActionsForMultipleTasks =
							this.loadingActionsForMultipleTasks.filter((s) => s !== 'delete');
						this.resetSelectedTasks();
					}
				};
				this.showConfirm('Delete tasks', 'Are you sure?', deleteMultipleTasks);
			},
			selectedGetter() {
				return this.selected;
			},
			selectingSetter(arr) {
				if (arr.filter((item) => item).length >= 2) {
					this.selecting = arr;
				}
			},
			selectedSetter(arr) {
				this.selected = arr.map((v, i) =>
					this.selected[i] && v
						? false
						: v && !this.selected[i]
						? true
						: !v && this.selected[i],
				);

				this.isShowSelectedTasksCommonTime =
					this.selected.filter(Boolean).length >= 1;
				this.selecting = [];
				if (this.isShowSelectedTasksCommonTime) {
					this.countTimeForModal();
				}
			},
			countTimeForModal() {
				const time = this.getSelectedTasks().reduce(
					(p, c) => p + c.common_time,
					0,
				);
				this.timeForModal = this.formatTime(time);
			},
			getSelectedTasks() {
				return this.tasks.filter((task, index) => this.selected[index]);
			},
			resetSelectedTasks() {
				this.isShowSelectedTasksCommonTime = false;
				this.selected = [];
				this.selecting = [];
			},
			closeTimeInModal() {
				this.showTimeInModal = false;
				this.isShowSelectedTasksCommonTime = false;
				this.resetSelectedTasks();
			},
			async exportSelectedTasks(exportType = 'csv') {
				this.loadingActionsForMultipleTasks.push(exportType);
				const tasksIds = this.getSelectedTasks().map(({ id }) => id);

				try {
					const data = await exportTasks(exportType, {
						ids: tasksIds,
						per_hour: 1000,
					});

					downloadFile(data, 'export.' + exportType);
				} catch (e) {
					console.error(e);
				} finally {
					this.loadingActionsForMultipleTasks =
						this.loadingActionsForMultipleTasks.filter(
							(type) => type !== exportType,
						);
				}
			},
			selectAll() {
				this.selected = this.tasks.map(() => true);
				this.isShowSelectedTasksCommonTime =
					this.selected.filter(Boolean).length > 1;

				this.countTimeForModal();
			},
			onPageChange(page: number) {
				if (page < 1 || page > this.pagination.last_page) return;
				this.$emit('page-change', page);
				this.resetSelectedTasks();
			},
			onPerPageChange() {
				if (this.perPage === this.pagination.per_page) return;
				this.$emit('per-page-change', this.perPage);
				this.resetSelectedTasks();
			}
		},
	};
</script>
