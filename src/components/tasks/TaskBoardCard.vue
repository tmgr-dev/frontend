<template>
	<div
		:class="{
			'border-l-8 border-solid border-green-600 dark:border-green-500': task.start_time,
		}"
		class="rounded border border-gray-200 bg-gray-100 px-3 pb-5 pt-3 shadow dark:border-gray-700 dark:bg-gray-800"
	>
		<div class="flex justify-between gap-3">
			<a
				class="w-44 break-words font-sans text-sm font-semibold tracking-wide text-tmgr-blue dark:text-tmgr-gray"
				:href="getTaskUrl(task)"
				@click.prevent="$store.commit('setCurrentTaskIdForModal', task.id)"
			>
				{{ task.title }}
			</a>

			<div class="flex items-start gap-2">
				<AssigneeUsers
					:assignees="task.assignees"
					:show-assignee-controls="false"
					avatarsClass="h-6 w-6"
				/>

				<DropdownMenu>
					<DropdownMenuTrigger as-child>
						<button
							class="flex items-center justify-center rounded p-0.5 text-gray-500 hover:bg-gray-200 hover:text-gray-700 dark:text-gray-400 dark:hover:bg-gray-700 dark:hover:text-gray-200"
							@click.stop
						>
							<MoreVertical class="h-4 w-4" />
						</button>
					</DropdownMenuTrigger>

					<DropdownMenuContent align="end" class="w-48">
						<DropdownMenuItem @click="handleOpenModal">
							<Eye class="mr-2 h-4 w-4" />
							<span>Open details</span>
						</DropdownMenuItem>
						<DropdownMenuSeparator />
						<DropdownMenuItem @click="handleMoveToTop">
							<ArrowUpToLine class="mr-2 h-4 w-4" />
							<span>Move to top</span>
						</DropdownMenuItem>
						<DropdownMenuItem @click="handleMoveToBottom">
							<ArrowDownToLine class="mr-2 h-4 w-4" />
							<span>Move to bottom</span>
						</DropdownMenuItem>
						<DropdownMenuSeparator />
						<DropdownMenuItem @click="handleArchive">
							<ArchiveIcon class="mr-2 h-4 w-4" />
							<span>Archive</span>
						</DropdownMenuItem>
						<DropdownMenuItem @click="handleDelete" class="text-red-600 dark:text-red-400">
							<Trash2 class="mr-2 h-4 w-4" />
							<span>Delete</span>
						</DropdownMenuItem>
					</DropdownMenuContent>
				</DropdownMenu>
			</div>
		</div>

		<div class="mt-4 flex items-center justify-between">
			<div class="flex items-center gap-2">
				<span
					:class="task.start_time ? 'text-green-600 dark:text-green-400' : 'text-orange-600 dark:text-orange-400'"
					class="material-icons text-sm"
				>
					alarm
				</span>
				<span class="text-sm text-gray-600 dark:text-gray-400">
					{{ secondsToHumanReadableString(displayTask.common_time) }}
				</span>
				<button
					v-if="!task.start_time"
					v-tooltip.top="setTooltipData('Start timer')"
					:disabled="isLoadingTimer"
					class="flex items-center justify-center rounded bg-green-500/80 p-0.5 text-white hover:bg-green-500 disabled:opacity-50 dark:bg-green-600/70 dark:hover:bg-green-600/90"
					@click.stop="handleStartTimer"
				>
					<span v-if="!isLoadingTimer" class="material-icons text-sm leading-none">play_arrow</span>
					<Loader v-else is-mini />
				</button>
				<button
					v-else
					v-tooltip.top="setTooltipData('Stop timer')"
					:disabled="isLoadingTimer"
					class="flex items-center justify-center rounded bg-red-500/80 p-0.5 text-white hover:bg-red-500 disabled:opacity-50 dark:bg-red-600/70 dark:hover:bg-red-600/90"
					@click.stop="handleStopTimer"
				>
					<span v-if="!isLoadingTimer" class="material-icons text-sm leading-none">stop</span>
					<Loader v-else is-mini />
				</button>
			</div>

			<CategoryBadge
				class="flex-row-reverse"
				:category="task.category"
				:status-id="task.status_id"
			/>
		</div>
	</div>
</template>

<script>
	import Badge from '../general/Badge.vue';
	import TimePreparationMixin from '@/mixins/TimePreparationMixin';
	import TasksListMixin from '@/mixins/TasksListMixin';
	import SetTooltipData from '@/mixins/SetTooltipData';
	import CategoryBadge from '@/components/general/CategoryBadge.vue';
	import AssigneeAvatar from '@/components/general/AssigneeAvatar.vue';
	import AssigneeUsers from '@/components/general/AssigneeUsers.vue';
	import Loader from '@/components/loaders/Loader.vue';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuItem,
		DropdownMenuSeparator,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';
	import { MoreVertical, ArrowUpToLine, ArrowDownToLine, Trash2, ArchiveIcon, Eye } from 'lucide-vue-next';
	import { mapState } from 'vuex';
	import { generateTaskUrl } from '@/utils/url';
	import { startTaskTimeCounter, stopTaskTimeCounter, updateTaskStatus, deleteTask } from '@/actions/tmgr/tasks';
	import { getStatuses } from '@/actions/tmgr/statuses';

	export default {
		mixins: [TimePreparationMixin, TasksListMixin, SetTooltipData],
		components: {
			AssigneeUsers,
			AssigneeAvatar,
			CategoryBadge,
			Badge,
			Loader,
			DropdownMenu,
			DropdownMenuContent,
			DropdownMenuItem,
			DropdownMenuSeparator,
			DropdownMenuTrigger,
			MoreVertical,
			ArrowUpToLine,
			ArrowDownToLine,
			Trash2,
			ArchiveIcon,
			Eye,
		},
		emits: ['timer-started', 'timer-stopped', 'move-to-top', 'move-to-bottom', 'task-deleted', 'task-archived'],
		props: {
			task: {
				type: Object,
				default: () => ({}),
			},
		},
		data() {
			return {
				timerInterval: null,
				currentDisplayTime: 0,
				isLoadingTimer: false,
				statuses: [],
			};
		},
		computed: {
			...mapState({
				workspaces: state => state.workspaces || [],
				currentWorkspaceId: state => {
					const setting = state.user?.settings?.find(
						setting => setting.key === 'current_workspace'
					);
					return setting ? setting.value : null;
				}
			}),
			currentWorkspace() {
				return this.workspaces.find(workspace => workspace.id === this.currentWorkspaceId);
			},
			badgeColor() {
				const mappings = {
					Design: 'purple',
					'Feature Request': 'teal',
					Backend: 'blue',
					QA: 'green',
					default: 'black',
				};
				return mappings[this.task.type] || mappings.default;
			},
			user() {
				return this.$store.state.user;
			},
			displayTask() {
				if (!this.task.start_time) {
					return this.task;
				}
				return {
					...this.task,
					common_time: this.currentDisplayTime,
				};
			},
		},
		watch: {
			task: {
				handler(newTask) {
					this.currentDisplayTime = newTask.common_time || 0;
					if (newTask.start_time) {
						if (!this.timerInterval) {
							this.startTimer();
						}
					} else {
						this.stopTimer();
					}
				},
				deep: true,
				immediate: true,
			},
			'task.start_time'(newVal, oldVal) {
				if (newVal && !oldVal) {
					this.startTimer();
				} else if (!newVal && oldVal) {
					this.stopTimer();
				}
			},
			'task.common_time'() {
				if (!this.task.start_time) {
					this.currentDisplayTime = this.task.common_time || 0;
				}
			},
		},
		async created() {
			try {
				this.statuses = await getStatuses();
			} catch (e) {
				console.error('Failed to load statuses:', e);
			}
		},
		methods: {
			getTaskUrl(task) {
				return generateTaskUrl(
					task.id,
					this.currentWorkspace,
					task.category && typeof task.category === 'object' ? task.category : null
				);
			},
			async handleStartTimer() {
				if (!this.task.id || this.isLoadingTimer) return;
				
				this.isLoadingTimer = true;
				try {
					const updatedTask = await startTaskTimeCounter(this.task.id);
					Object.assign(this.task, updatedTask);
					this.$store.commit('incrementReloadTasksKey');
					this.$emit('timer-started', updatedTask);
					
					const taskStatus = this.statuses.find(s => s.id === this.task.status_id);
					if (taskStatus && taskStatus.type === 'default') {
						const activeStatus = this.statuses.find(s => s.type === 'active');
						if (activeStatus) {
							const shouldSwitch = confirm(
								`Task "${this.task.title}" is in backlog. Switch to "${activeStatus.name}" status?`
							);
							if (shouldSwitch) {
								await updateTaskStatus(this.task.id, activeStatus.id);
								this.task.status_id = activeStatus.id;
								this.$store.commit('incrementReloadTasksKey');
							}
						}
					}
				} catch (e) {
					console.error('Failed to start timer:', e);
				} finally {
					this.isLoadingTimer = false;
				}
			},
			async handleStopTimer() {
				if (!this.task.id || this.isLoadingTimer) return;
				
				this.isLoadingTimer = true;
				try {
					const updatedTask = await stopTaskTimeCounter(this.task.id);
					Object.assign(this.task, updatedTask);
					this.$store.commit('incrementReloadTasksKey');
					this.$emit('timer-stopped', updatedTask);
				} catch (e) {
					console.error('Failed to stop timer:', e);
				} finally {
					this.isLoadingTimer = false;
				}
			},
			updateTimer() {
				if (!this.task.start_time) {
					return;
				}
				const now = Math.floor(Date.now() / 1000);
				const elapsed = now - this.task.start_time;
				this.currentDisplayTime = (this.task.common_time || 0) + elapsed;
			},
			startTimer() {
				if (this.timerInterval) {
					clearInterval(this.timerInterval);
				}
				this.updateTimer();
				this.timerInterval = setInterval(() => {
					this.updateTimer();
				}, 1000);
			},
			stopTimer() {
				if (this.timerInterval) {
					clearInterval(this.timerInterval);
					this.timerInterval = null;
				}
				this.currentDisplayTime = this.task.common_time || 0;
			},
			handleOpenModal() {
				this.$store.commit('setCurrentTaskIdForModal', this.task.id);
			},
			handleMoveToTop() {
				this.$emit('move-to-top', this.task);
			},
			handleMoveToBottom() {
				this.$emit('move-to-bottom', this.task);
			},
			async handleArchive() {
				if (!confirm(`Are you sure you want to archive "${this.task.title}"?`)) {
					return;
				}
				
				try {
					const archiveStatus = this.statuses.find(s => s.type === 'archived');
					if (!archiveStatus) {
						alert('Archive status not found. Please create an archive status first.');
						return;
					}
					
					await updateTaskStatus(this.task.id, archiveStatus.id);
					this.$emit('task-archived', this.task);
					this.$store.commit('incrementReloadTasksKey');
				} catch (e) {
					console.error('Failed to archive task:', e);
					alert('Failed to archive task. Please try again.');
				}
			},
			async handleDelete() {
				if (!confirm(`Are you sure you want to delete "${this.task.title}"?`)) {
					return;
				}
				
				try {
					await deleteTask(this.task.id);
					this.$emit('task-deleted', this.task);
					this.$store.commit('incrementReloadTasksKey');
				} catch (e) {
					console.error('Failed to delete task:', e);
					alert('Failed to delete task. Please try again.');
				}
			},
		},
		beforeUnmount() {
			this.stopTimer();
		},
	};
</script>
