<template>
	<div class="relative w-full items-center justify-center">
		<TasksMultipleActionsModal
			v-if="isShowSelectedTasksCommonTime"
			:is-loading-actions="loadingActionsForMultipleTasks"
			:status="status"
			@close="closeTimeInModal"
			@export="exportSelectedTasks"
			@remove="deleteSelectedTasks"
			@updateStatus="updateStatusForSelectedTasks"
		>
			<p class="text-center text-gray-800 dark:text-gray-200">
				<b>Selected tasks: </b>{{ selected.filter(Boolean).length }}<br />
				{{ timeForModal }}
			</p>
		</TasksMultipleActionsModal>

		<div v-if="hasSelectable" class="flex items-center gap-2 px-2 pb-2">
			<button
				@click="selectAll"
				type="button"
				class="h-9 rounded-pill border border-line bg-surface px-4 text-sm font-medium text-ink hover:bg-surface-hover"
			>
				{{ allSelected ? 'Deselect all' : 'Select all' }}
			</button>
			<button
				v-if="selected.filter(Boolean).length > 0"
				@click="showProcessModal"
				type="button"
				class="h-9 rounded-pill bg-status-done px-4 text-sm font-medium text-white shadow-tmgr-xs hover:opacity-90"
			>
				Process ({{ selected.filter(Boolean).length }})
			</button>
			<span
				v-if="selected.filter(Boolean).length > 0"
				class="text-sm text-ink-subtle"
			>
				{{ selectedSummary }}
			</span>
		</div>

		<div
			v-selectable="
				hasSelectable ? { selectedGetter, selectedSetter, selectingSetter } : {}
			"
			class="relative border pb-2"
			:class="[
				hasSelectable
					? 'rounded-card border-dashed border-brand/40 px-2'
					: 'border-transparent',
			]"
		>
			<div
				class="border-black dark:border-gray-400"
				:class="[hasSelectable ? 'selection' : '']"
			/>

			<div
				v-for="(task, i) in tasks"
				:key="task.id"
				v-memo="[task.id, task.common_time, task.start_time, task.status_id, task.deleted_at, selected[i], selecting[i], hoveredTaskId === task.id, assigneePopoverOpen[task.id]]"
				:class="{
					selected: !!selected[i],
					selecting: !!selecting[i],
					'opacity-50 hover:opacity-100': task.deleted_at,
				}"
				:data-task-id="task.id"
				:draggable="false"
				class="selectable relative mt-3 w-full"
			>
				<BounceLoader
					v-if="loadingActionTasksIds.includes(task.id)"
					class="l-0 t-0 absolute z-10"
				/>

				<button
					type="button"
					:class="{
					'border-l-[3px] border-l-status-done':
						task.start_time,
				}"
					class="group/list-item relative w-full overflow-hidden rounded-card border border-line bg-surface text-left shadow-tmgr-xs transition-all duration-150 hover:shadow-tmgr-md hover:border-line-strong md:flex"
					@click="$store.commit('setCurrentTaskIdForModal', task.id)"
					@mouseenter="hoveredTaskId = task.id"
					@mouseleave="hoveredTaskId = null"
				>
					<Popover
						v-if="isFeatureEnabled('task.assignees')"
						v-model:open="assigneePopoverOpen[task.id]"
					>
						<PopoverTrigger as-child>
							<button
								class="group/assignee absolute right-3 top-3 z-10 flex h-7 w-7 items-center justify-center rounded-pill text-ink-subtle transition-colors hover:bg-surface-hover hover:text-ink"
								@click.stop
								:title="
									task.assignees?.length
										? 'Change assignee'
										: 'Assign someone'
								"
							>
								<UserPlus
									v-if="
										assigneePopoverOpen[task.id] || !task.assignees?.length
									"
									class="h-6 w-6 rounded-pill bg-surface-sunken p-1"
								/>
								<template v-else>
									<UserPlus
										class="hidden h-6 w-6 rounded-pill bg-surface-sunken p-1 group-hover/assignee:block"
									/>
									<AssigneeUsers
										class="group-hover/assignee:hidden"
										:assignees="task.assignees"
										avatarsClass="h-6 w-6"
										:show-assignee-controls="false"
									/>
								</template>
							</button>
						</PopoverTrigger>
						<PopoverContent
							class="z-50 w-52 p-0"
							align="end"
							side="bottom"
							@click.stop
						>
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
												:class="[
													'mr-2 h-4 w-4',
													isAssignedTo(task, member.id)
														? 'opacity-100'
														: 'opacity-0',
												]"
											/>
											{{ member.name }}
										</CommandItem>
									</CommandGroup>
								</CommandList>
							</Command>
						</PopoverContent>
					</Popover>

					<div
						class="w-full px-4 py-3 transition-colors duration-150 group-hover/list-item:bg-surface-hover"
					>
						<div class="flex items-start gap-2 pr-9">
							<div
								v-if="draggable"
								class="task-drag-handle flex-shrink-0 cursor-grab touch-none select-none pt-0.5 text-ink-faint hover:text-ink-subtle active:cursor-grabbing"
								:draggable="true"
								@dragstart="onDragStart($event, task)"
								@click.stop
							>
								<span class="material-icons pointer-events-none text-base"
									>drag_indicator</span
								>
							</div>
							<div class="flex-1 min-w-0">
								<CategoryBadge
									v-if="showCategoryBadges && task.category"
									class="mb-2 shrink-0 self-start"
									:category="task.category"
								/>

							<div
								class="text-left text-base font-medium leading-snug text-ink"
								:title="task.title?.length > 60 ? task.title : ''"
							>
								{{ taskTitles[task.id] }}
							</div>
							</div>
						</div>

						<div
							v-if="isFeatureEnabled('task.countdown')"
							class="mt-3 flex items-center gap-2"
						>
						<AlarmClock
							class="h-4 w-4"
							:class="
								taskTimeExceeded[task.id]
									? 'text-status-fix'
									: 'text-status-done'
							"
						/>

						<span
							class="text-sm text-ink"
							:class="taskTimeExceeded[task.id] && 'text-status-fix-fg'"
						>
							{{ taskFormattedTimes[task.id] }}
						</span>

							<button
								v-if="!task.start_time"
								v-tooltip.top="setTooltipData('Start timer')"
								:disabled="isLoadingActions[`start-${task.id}`]"
								class="ml-1 flex h-5 w-5 items-center justify-center rounded bg-status-done text-white shadow-tmgr-xs transition-opacity hover:opacity-90 disabled:opacity-50"
								@click.stop="startCountdown(task, `start-${task.id}`)"
							>
								<Play
									v-if="!isLoadingActions[`start-${task.id}`]"
									class="h-3 w-3 fill-current"
								/>
								<Loader v-else is-mini />
							</button>
							<button
								v-else
								v-tooltip.top="setTooltipData('Stop timer')"
								:disabled="isLoadingActions[`stop-${task.id}`]"
								class="ml-1 flex h-5 w-5 items-center justify-center rounded bg-status-fix text-white shadow-tmgr-xs transition-opacity hover:opacity-90 disabled:opacity-50"
								@click.stop="stopCountdown(task, `stop-${task.id}`)"
							>
								<Square
									v-if="!isLoadingActions[`stop-${task.id}`]"
									class="h-2.5 w-2.5 fill-current"
								/>
								<Loader v-else is-mini />
							</button>
						</div>

						<div
							class="mt-3 flex items-center gap-x-3 text-2xs text-ink-faint"
						>
						<TaskTimeInfo
							:created-at="task.created_at"
							:updated-at="task.updated_at"
							:overtime="taskOvertimes[task.id]"
						/>

						<span class="ml-auto flex items-center gap-1.5 text-ink-subtle">
							<span>{{ taskStatusNames[task.id] }}</span>
							<span
								class="inline-block h-2 w-2 rounded-full"
								:style="{ backgroundColor: taskStatusColors[task.id] }"
							/>
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
			<div
				v-if="pagination.total > pagination.per_page"
				class="mt-6 flex items-center justify-between px-2"
			>
				<div class="flex items-center gap-3">
					<span class="text-sm text-ink-subtle">
						Showing {{ pagination.from }} to {{ pagination.to }} of
						{{ pagination.total }} tasks
					</span>

					<select
						v-model="perPage"
						@change="onPerPageChange"
						class="h-9 rounded-pill border border-line bg-surface px-3 text-sm text-ink outline-none focus:border-line-strong"
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
						class="h-9 rounded-pill border border-line bg-surface px-4 text-sm text-ink hover:bg-surface-hover disabled:opacity-40"
					>
						Previous
					</button>

					<span class="px-2 text-sm text-ink-subtle">
						Page {{ pagination.current_page }} of {{ pagination.last_page }}
					</span>

					<button
						:disabled="pagination.current_page === pagination.last_page"
						@click="onPageChange(pagination.current_page + 1)"
						class="h-9 rounded-pill border border-line bg-surface px-4 text-sm text-ink hover:bg-surface-hover disabled:opacity-40"
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
		@onCancel="
			confirm = undefined;
			backlogStatusChangeConfirm = null;
		"
		@onOk="
			confirm.action();
			confirm = undefined;
			backlogStatusChangeConfirm = null;
		"
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
		deleteTask,
		startTaskTimeCounter,
		stopTaskTimeCounter,
		updateTaskStatus,
		updateTaskPartially,
		Task,
		PaginationMeta,
	} from '@/actions/tmgr/tasks';
	import { getStatuses, Status } from '@/actions/tmgr/statuses';
	import {
		getWorkspaceMembers,
		WorkspaceMember,
	} from '@/actions/tmgr/workspaces';
	import CategoryBadge from '@/components/general/CategoryBadge.vue';
	import Button from '@/components/general/Button.vue';
	import { PropType } from 'vue';
	import AssigneeUsers from '@/components/general/AssigneeUsers.vue';
	import {
		Popover,
		PopoverContent,
		PopoverTrigger,
	} from '@/components/ui/popover';
	import {
		Command,
		CommandEmpty,
		CommandGroup,
		CommandInput,
		CommandItem,
		CommandList,
	} from '@/components/ui/command';
	import { UserPlus, Check, ClockPlus, Play, Square, AlarmClock } from 'lucide-vue-next';
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import TaskTimeInfo from '@/components/tasks/TaskTimeInfo.vue';

	export default {
		name: 'TasksListComponent',
		components: {
			TaskTimeInfo,
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
			ClockPlus,
			Play,
			Square,
			AlarmClock,
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
					to: 0,
				}),
			},
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
				},
			},
			assigneePopoverOpen: {
				deep: true,
				handler(newVal) {
					if (Object.values(newVal).some((v) => v)) {
						this.loadWorkspaceMembers();
					}
				},
			},
		},
		mixins: [TasksListMixin, TaskActionsInTheListMixin, SetTooltipData],
		data() {
			const { isFeatureEnabled } = useFeatureToggles();
			return {
				isFeatureEnabled,
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
				backlogStatusChangeConfirm: null as {
					task: Task;
					dotId: string | null;
				} | null,
				hoveredTaskId: null as number | null,
				assigneePopoverOpen: {} as Record<number, boolean>,
				workspaceMembers: [] as WorkspaceMember[],
			};
		},
		async created() {
			try {
				this.statuses = await getStatuses();
			} catch (e) {
				console.error('Failed to load statuses:', e);
			}
		},
	computed: {
		allSelected() {
			return (
				this.tasks.length > 0 &&
				this.selected.length === this.tasks.length &&
				this.selected.every(Boolean)
			);
		},
		selectedSummary() {
			const tasks = this.getSelectedTasks();
			if (!tasks.length) return '';
			const totalSeconds = tasks.reduce((s, t) => s + (t.common_time || 0), 0);
			const totalHours = (totalSeconds / 3600).toFixed(1);
			let overtimeSeconds = 0;
			for (const task of tasks) {
				const expected = this.getApproximatelyTime(task);
				const actual = task.common_time || 0;
				if (expected > 0 && actual > expected) {
					overtimeSeconds += actual - expected;
				}
			}
			const overtimeHours = (overtimeSeconds / 3600).toFixed(1);
			let text = `${tasks.length} of ${this.tasks.length} · ${totalHours}h`;
			if (overtimeSeconds > 0) {
				text += ` · +${overtimeHours}h overtime`;
			}
			return text;
		},
		taskComputedData() {
			return this.tasks.reduce((acc, task) => {
				acc[task.id] = {
					title: this.truncateTitle(task.title),
					timeExceeded: this.isTimeExceeded(task),
					formattedTime: this.getTaskFormattedTime(task),
					overtime: this.getTaskOvertime(task),
					statusName: this.getStatusName(task),
					statusColor: this.getStatusColor(task),
				};
				return acc;
			}, {} as Record<number, { title: string; timeExceeded: boolean; formattedTime: string; overtime: string | null; statusName: string; statusColor: string }>);
		},
		taskTitles() {
			const data: Record<number, string> = {};
			for (const id in this.taskComputedData) {
				data[id] = this.taskComputedData[id].title;
			}
			return data;
		},
		taskTimeExceeded() {
			const data: Record<number, boolean> = {};
			for (const id in this.taskComputedData) {
				data[id] = this.taskComputedData[id].timeExceeded;
			}
			return data;
		},
		taskFormattedTimes() {
			const data: Record<number, string> = {};
			for (const id in this.taskComputedData) {
				data[id] = this.taskComputedData[id].formattedTime;
			}
			return data;
		},
		taskOvertimes() {
			const data: Record<number, string | null> = {};
			for (const id in this.taskComputedData) {
				data[id] = this.taskComputedData[id].overtime;
			}
			return data;
		},
		taskStatusNames() {
			const data: Record<number, string> = {};
			for (const id in this.taskComputedData) {
				data[id] = this.taskComputedData[id].statusName;
			}
			return data;
		},
		taskStatusColors() {
			const data: Record<number, string> = {};
			for (const id in this.taskComputedData) {
				data[id] = this.taskComputedData[id].statusColor;
			}
			return data;
		},
	},
		methods: {
			truncateTitle(title: string) {
				if (!title) return '';
				return title.length > 60 ? title.substring(0, 60) + '...' : title;
			},
			async loadWorkspaceMembers() {
				if (this.workspaceMembers.length > 0) return;
				try {
					const workspaceId = this.$store.state.user?.settings?.find(
						(s: any) => s.key === 'current_workspace',
					)?.value;
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
					newAssigneeIds = currentAssigneeIds.filter(
						(id: number) => id !== memberId,
					);
				} else {
					newAssigneeIds = [...currentAssigneeIds, memberId];
				}

				try {
					await updateTaskPartially(task.id!, {
						assignees: newAssigneeIds,
					} as any);
					const member = this.workspaceMembers.find(
						(m: WorkspaceMember) => m.id === memberId,
					);
					if (currentAssigneeIds.includes(memberId)) {
						task.assignees = task.assignees?.filter(
							(a: any) => a.id !== memberId,
						);
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
				if (!task?.id) return;
				if (dotId) this.isLoadingActions[dotId] = true;
				try {
					const updatedTask = await stopTaskTimeCounter(task.id);
					if (updatedTask && typeof updatedTask === 'object') {
						Object.assign(task, updatedTask);
						this.$store.commit('updateSingleTask', updatedTask);
					} else {
						task.start_time = 0;
					}
					this.$emit('reload-tasks');
				} catch (e) {
					console.error('Failed to stop timer:', e);
				} finally {
					if (dotId) this.isLoadingActions[dotId] = false;
				}
			},
			async startCountdown(task: Task, dotId: string | null) {
				if (!task.id) return;

				if (dotId) {
					this.isLoadingActions[dotId] = true;
				}
				try {
					const updatedTask = await startTaskTimeCounter(task.id);
					if (updatedTask && typeof updatedTask === 'object') {
						Object.assign(task, updatedTask);
						this.$store.commit('updateSingleTask', updatedTask);
					} else {
						task.start_time = Math.floor(Date.now() / 1000);
					}
					this.$emit('reload-tasks');

					const taskStatus = this.statuses.find((s) => s.id === task.status_id);
					if (taskStatus && taskStatus.type === 'default') {
						this.backlogStatusChangeConfirm = { task, dotId };
						this.showBacklogStatusChangeConfirm();
					}
				} catch (e) {
					console.error('Failed to start timer:', e);
				} finally {
					if (dotId) this.isLoadingActions[dotId] = false;
				}
			},
			showBacklogStatusChangeConfirm() {
				if (!this.backlogStatusChangeConfirm) return;

				const { task, dotId } = this.backlogStatusChangeConfirm;
				const activeStatus = this.statuses.find((s) => s.type === 'active');

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

						const { task: confirmTask, dotId: confirmDotId } =
							this.backlogStatusChangeConfirm;
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
					},
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
					deleteTask: this.status === 'hidden',
				};
			},
			getApproximatelyTime(task: Task) {
				if (task.approximately_time) {
					return parseInt(String(task.approximately_time), 10);
				}
				const setting = task.settings?.find(
					(s: any) => s.key === 'approximately_time',
				);
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
			getTaskOvertime(task: Task) {
				const approximatelyTime = this.getApproximatelyTime(task);
				if (!approximatelyTime || approximatelyTime <= 0) {
					return null;
				}
				const currentTime = task.common_time || 0;
				const overtime = currentTime - approximatelyTime;
				if (overtime <= 0) {
					return null;
				}
				const hours = Math.floor(overtime / 3600);
				const minutes = Math.floor((overtime % 3600) / 60);
				const parts = [];
				if (hours > 0) {
					parts.push(`${hours}h`);
				}
				if (minutes > 0) {
					parts.push(`${minutes}m`);
				}
				return parts.length > 0 ? parts.join(' ') : null;
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
					hour12: false,
				});
			},
			deleteSelectedTasks() {
				const deleteMultipleTasks = async () => {
					try {
						this.loadingActionsForMultipleTasks.push('delete');
						const selectedTasks = this.tasks.filter((_, i) => this.selected[i]);
						const deletePromises = selectedTasks.map(async (task) => {
							const deletedAt = await deleteTask(task.id);
							task.deleted_at = deletedAt;
							return task;
						});
						await Promise.all(deletePromises);
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

				this.selecting = [];
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
			async exportSelectedTasks(payload = {}) {
				const exportType = typeof payload === 'string' ? payload : (payload.type || 'csv');
				const settings = typeof payload === 'string' ? {} : (payload.settings || {});

				this.loadingActionsForMultipleTasks.push(exportType);
				const tasksIds = this.getSelectedTasks().map(({ id }) => id);

				try {
					const data = await exportTasks(exportType, {
						ids: tasksIds,
						per_hour: 1000,
						...settings,
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
			showProcessModal() {
				this.countTimeForModal();
				this.isShowSelectedTasksCommonTime = true;
			},
			selectAll() {
				if (this.allSelected) {
					this.selected = [];
					this.isShowSelectedTasksCommonTime = false;
				} else {
					this.selected = this.tasks.map(() => true);
				}
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
			},
		},
	};
</script>
