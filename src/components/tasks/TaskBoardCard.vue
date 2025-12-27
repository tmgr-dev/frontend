<template>
	<div
		:class="{
			'border-l-8 border-solid border-green-600 dark:border-green-500': task.start_time,
			'border-b-4 border-b-red-500 dark:border-b-red-400': isTimeExceeded,
		}"
		class="rounded border border-gray-200 bg-gray-100 px-3 pt-3 shadow dark:border-gray-700 dark:bg-gray-800"
		@mouseenter="isHovered = true"
		@mouseleave="isHovered = false"
	>
		<div class="flex items-center justify-between mb-2">
			<div class="flex items-center gap-2">
				<div class="task-drag-handle flex items-center justify-center cursor-grab active:cursor-grabbing text-gray-400 hover:text-gray-600 dark:text-gray-500 dark:hover:text-gray-300 select-none touch-none">
					<span class="material-icons text-sm pointer-events-none">drag_indicator</span>
				</div>

				<template v-if="displayTask.common_time > 0">
					<span
						:class="isTimeExceeded ? 'text-red-600 dark:text-red-400' : 'text-green-600 dark:text-green-400'"
						class="material-icons text-sm"
					>
						alarm
					</span>
					<span class="text-xs text-gray-600 dark:text-gray-400">
						{{ secondsToHumanReadableString(displayTask.common_time) }}
					</span>
				</template>
				
				<button
					v-if="!task.start_time"
					v-tooltip.top="setTooltipData('Start timer')"
					:disabled="isLoadingTimer"
					class="flex items-center justify-center rounded bg-green-500/80 p-0.5 text-white hover:bg-green-500 disabled:opacity-50 dark:bg-green-600/70 dark:hover:bg-green-600/90"
					@click.stop="handleStartTimer"
				>
					<span v-if="!isLoadingTimer" class="material-icons text-xs leading-none">play_arrow</span>
					<Loader v-else is-mini />
				</button>
				<button
					v-else
					v-tooltip.top="setTooltipData('Stop timer')"
					:disabled="isLoadingTimer"
					class="flex items-center justify-center rounded bg-red-500/80 p-0.5 text-white hover:bg-red-500 disabled:opacity-50 dark:bg-red-600/70 dark:hover:bg-red-600/90"
					@click.stop="handleStopTimer"
				>
					<span v-if="!isLoadingTimer" class="material-icons text-xs leading-none">stop</span>
					<Loader v-else is-mini />
				</button>
			</div>

			<div class="flex items-center gap-2">
				<Popover v-model:open="showAssigneePopover">
					<PopoverTrigger as-child>
						<button
							v-show="task.assignees?.length || isHovered || showAssigneePopover"
							class="flex items-center justify-center rounded p-0.5 text-gray-500 hover:bg-gray-200 hover:text-gray-700 dark:text-gray-400 dark:hover:bg-gray-700 dark:hover:text-gray-200 group"
							@click.stop
							:title="task.assignees?.length ? 'Change assignee' : 'Assign someone'"
						>
							<UserPlus 
								v-if="showAssigneePopover || !task.assignees?.length" 
								class="h-4 w-4" 
							/>
							<template v-else>
								<UserPlus class="hidden h-4 w-4 group-hover:block" />
								<AssigneeUsers
									class="group-hover:hidden"
									:assignees="task.assignees"
									:show-assignee-controls="false"
									avatarsClass="h-4 w-4"
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
										@select="toggleAssignee(member.id)"
										class="cursor-pointer"
									>
										<Check
											:class="['mr-2 h-4 w-4', isAssigned(member.id) ? 'opacity-100' : 'opacity-0']"
										/>
										{{ member.name }}
									</CommandItem>
								</CommandGroup>
							</CommandList>
						</Command>
					</PopoverContent>
				</Popover>

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

		<a
			class="block break-words font-sans text-sm font-semibold tracking-wide text-tmgr-blue dark:text-tmgr-gray"
			:href="getTaskUrl(task)"
			:title="task.title?.length > 60 ? task.title : ''"
			@click.prevent="$store.commit('setCurrentTaskIdForModal', task.id)"
		>
			{{ truncateTitle(task.title) }}
		</a>

		<div class="mt-2">
			<CategoryBadge
				class="flex-row-reverse"
				:category="task.category"
				:status-id="task.status_id"
			/>
		</div>

		<div class="mt-2 flex flex-wrap gap-x-2 pb-2 text-[9px] text-gray-400/60 dark:text-gray-500/60">
			<span v-if="task.created_at" :title="'Created: ' + formatFullDate(task.created_at)">
				{{ formatRelativeTime(task.created_at) }}
			</span>
			<span v-if="task.updated_at && task.updated_at !== task.created_at" class="flex items-center gap-0.5" :title="'Edited: ' + formatFullDate(task.updated_at)">
				<span class="material-icons" style="font-size: 10px;">edit</span>
				{{ formatRelativeTime(task.updated_at) }}
			</span>
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
	import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
	import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from '@/components/ui/command';
	import { MoreVertical, ArrowUpToLine, ArrowDownToLine, Trash2, ArchiveIcon, Eye, UserPlus, Check } from 'lucide-vue-next';
	import { mapState } from 'vuex';
	import { getWorkspaceMembers } from '@/actions/tmgr/workspaces';
	import { generateTaskUrl } from '@/utils/url';
	import { formatRelativeTime } from '@/utils/timeUtils';
	import { startTaskTimeCounter, stopTaskTimeCounter, updateTaskStatus, updateTaskPartially, deleteTask } from '@/actions/tmgr/tasks';

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
			Popover,
			PopoverContent,
			PopoverTrigger,
			Command,
			CommandEmpty,
			CommandGroup,
			CommandInput,
			CommandItem,
			CommandList,
			MoreVertical,
			ArrowUpToLine,
			ArrowDownToLine,
			Trash2,
			ArchiveIcon,
			Eye,
			UserPlus,
			Check,
		},
	emits: ['timer-started', 'timer-stopped', 'move-to-top', 'move-to-bottom', 'task-deleted', 'task-archived'],
	props: {
		task: {
			type: Object,
			default: () => ({}),
		},
		statuses: {
			type: Array,
			default: () => [],
		},
	},
	data() {
		return {
			timerInterval: null,
			currentDisplayTime: 0,
			isLoadingTimer: false,
			isHovered: false,
			showAssigneePopover: false,
			workspaceMembers: [],
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
			isTimeExceeded() {
				const approximatelyTime = this.getApproximatelyTime();
				if (!approximatelyTime || approximatelyTime <= 0) {
					return false;
				}
				const currentTime = this.displayTask.common_time || 0;
				return currentTime > approximatelyTime;
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
			showAssigneePopover(newVal) {
				if (newVal) {
					this.loadWorkspaceMembers();
				}
			},
		},
	methods: {
			truncateTitle(title) {
				if (!title) return '';
				return title.length > 60 ? title.substring(0, 60) + '...' : title;
			},
			async loadWorkspaceMembers() {
				if (this.workspaceMembers.length > 0) return;
				try {
					const workspaceId = this.currentWorkspaceId || this.$store.state.user?.settings?.find(s => s.key === 'current_workspace')?.value;
					if (workspaceId) {
						this.workspaceMembers = await getWorkspaceMembers(workspaceId);
					}
				} catch (e) {
					console.error('Failed to load workspace members:', e);
				}
			},
			isAssigned(memberId) {
				return this.task.assignees?.some(a => a.id === memberId) || false;
			},
			async toggleAssignee(memberId) {
				const currentAssigneeIds = this.task.assignees?.map(a => a.id) || [];
				let newAssigneeIds;
				
				if (currentAssigneeIds.includes(memberId)) {
					newAssigneeIds = currentAssigneeIds.filter(id => id !== memberId);
				} else {
					newAssigneeIds = [...currentAssigneeIds, memberId];
				}
				
				try {
					await updateTaskPartially(this.task.id, { assignees: newAssigneeIds });
					const member = this.workspaceMembers.find(m => m.id === memberId);
					if (currentAssigneeIds.includes(memberId)) {
						this.task.assignees = this.task.assignees.filter(a => a.id !== memberId);
					} else if (member) {
						if (!this.task.assignees) this.task.assignees = [];
						this.task.assignees.push(member);
					}
				} catch (e) {
					console.error('Failed to update assignees:', e);
				}
			},
			getTaskUrl(task) {
				return generateTaskUrl(
					task.id,
					this.currentWorkspace,
					task.category && typeof task.category === 'object' ? task.category : null
				);
			},
			getApproximatelyTime() {
				if (this.task.approximately_time) {
					return parseInt(this.task.approximately_time, 10);
				}
				const setting = this.task.settings?.find(s => s.key === 'approximately_time');
				if (setting) {
					return parseInt(setting.value || setting.pivot?.value, 10);
				}
				return 0;
			},
			formatRelativeTime(dateString) {
				return formatRelativeTime(dateString);
			},
			formatFullDate(dateString) {
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
