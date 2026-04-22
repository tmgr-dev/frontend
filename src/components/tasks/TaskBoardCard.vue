<template>
	<div
		class="group relative flex flex-col rounded-card border border-line bg-surface px-[14px] py-[14px] font-display shadow-tmgr-xs transition-all duration-150 hover:shadow-tmgr-md"
		@mouseenter="isHovered = true"
		@mouseleave="isHovered = false"
	>
		<span
			class="pointer-events-none absolute left-0 top-[10px] bottom-[10px] w-[3px] rounded-full"
			:class="
				task.start_time
					? 'bg-status-done opacity-90'
					: 'bg-line-strong opacity-60'
			"
			aria-hidden="true"
		></span>

		<div class="mb-2 flex items-center gap-1.5 -ml-0.5">
			<div
				class="task-drag-handle flex h-[18px] w-[14px] cursor-grab touch-none select-none items-center justify-center text-ink-faint opacity-0 transition-opacity duration-150 hover:text-ink-subtle group-hover:opacity-100 active:cursor-grabbing"
				:title="'Drag to reorder'"
			>
				<GripVertical class="pointer-events-none h-3.5 w-3.5" />
			</div>

			<button
				v-if="task.start_time"
				v-tooltip.top="setTooltipData('Stop timer')"
				:disabled="isLoadingTimer"
				class="inline-flex h-[22px] items-center gap-1.5 rounded-pill bg-status-done-bg px-2 text-2xs font-semibold tabular-nums text-status-done-fg transition-colors hover:opacity-90 disabled:opacity-50"
				@click.stop="handleStopTimer"
			>
				<Loader v-if="isLoadingTimer" is-mini />
				<template v-else>
					<span
						class="h-[5px] w-[5px] rounded-full bg-status-done animate-tmgr-pulse"
					></span>
					<Pause class="h-2.5 w-2.5 fill-current" />
					<span>{{ formattedDisplayTime }}</span>
				</template>
			</button>
			<button
				v-else
				v-tooltip.top="setTooltipData('Start timer')"
				:disabled="isLoadingTimer"
				class="inline-flex h-[22px] items-center gap-1 rounded-pill bg-surface-sunken px-2 text-2xs font-semibold tabular-nums text-ink-muted transition-colors hover:bg-line disabled:opacity-50"
				:class="{ 'text-status-fix-fg': isTimeExceeded }"
				@click.stop="handleStartTimer"
			>
				<Loader v-if="isLoadingTimer" is-mini />
				<template v-else>
					<Play class="h-2.5 w-2.5 fill-current" />
					<span v-if="displayTask.common_time > 0">
						{{ formattedDisplayTime }}
					</span>
					<span v-else>0m</span>
				</template>
			</button>

			<span class="flex-1"></span>

			<Popover
				v-if="isFeatureEnabled('task.assignees')"
				v-model:open="showAssigneePopover"
			>
				<PopoverTrigger as-child>
					<button
						v-show="task.assignees?.length || isHovered || showAssigneePopover"
						class="group/assignee flex items-center justify-center rounded-pill text-ink-subtle transition-colors hover:text-ink"
						@click.stop
						:title="
							task.assignees?.length ? 'Change assignee' : 'Assign someone'
						"
					>
						<UserPlus
							v-if="showAssigneePopover || !task.assignees?.length"
							class="h-[22px] w-[22px] rounded-pill bg-surface-sunken p-1"
						/>
						<template v-else>
							<UserPlus
								class="hidden h-[22px] w-[22px] rounded-pill bg-surface-sunken p-1 group-hover/assignee:block"
							/>
							<AssigneeUsers
								class="group-hover/assignee:hidden"
								:assignees="task.assignees"
								:show-assignee-controls="false"
								avatarsClass="h-[22px] w-[22px]"
							/>
						</template>
					</button>
				</PopoverTrigger>
				<PopoverContent
					class="z-50 w-52 p-0"
					align="start"
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
									@select="toggleAssignee(member.id)"
									class="cursor-pointer"
								>
									<Check
										:class="[
											'mr-2 h-4 w-4',
											isAssigned(member.id) ? 'opacity-100' : 'opacity-0',
										]"
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
						class="flex h-[22px] w-[22px] items-center justify-center rounded-pill text-ink-subtle opacity-0 transition-opacity hover:bg-surface-hover hover:text-ink group-hover:opacity-100"
						@click.stop
					>
						<MoreVertical class="h-3.5 w-3.5" />
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
					<DropdownMenuItem
						@click="handleDelete"
						class="text-status-fix-fg"
					>
						<Trash2 class="mr-2 h-4 w-4" />
						<span>Delete</span>
					</DropdownMenuItem>
				</DropdownMenuContent>
			</DropdownMenu>
		</div>

		<a
			class="mb-2.5 block break-words text-sm font-semibold leading-snug tracking-tight text-ink hover:text-brand"
			:href="getTaskUrl(task)"
			:title="task.title"
			@click.prevent="$store.commit('setCurrentTaskIdForModal', task.id)"
		>
			{{ task.title }}
		</a>

		<div
			v-if="checklistProgress"
			class="mb-2.5 flex items-center gap-2"
		>
			<div class="h-1 flex-1 overflow-hidden rounded-full bg-surface-sunken">
				<div
					class="h-full rounded-full transition-all duration-300"
					:class="
						checklistProgress.percent === 100
							? 'bg-status-done'
							: 'bg-brand'
					"
					:style="{ width: `${checklistProgress.percent}%` }"
				></div>
			</div>
			<span class="text-2xs tabular-nums text-ink-subtle">
				{{ checklistProgress.checked }}/{{ checklistProgress.total }}
			</span>
		</div>

		<div class="flex flex-wrap items-center gap-1.5">
			<CategoryBadge
				v-if="task.category"
				:category="task.category"
				:status-id="task.status_id"
			/>

			<span class="flex-1"></span>

			<span
				v-if="formattedDate"
				class="flex items-center gap-1 text-2xs tabular-nums"
				:class="overtime ? 'text-status-fix-fg' : 'text-ink-subtle'"
			>
				<Clock class="h-3 w-3" />
				{{ formattedDate }}
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
	import {
		MoreVertical,
		ArrowUpToLine,
		ArrowDownToLine,
		Trash2,
		ArchiveIcon,
		Eye,
		UserPlus,
		Check,
		ClockPlus,
		FilePenLine,
		Siren,
		GripVertical,
		Play,
		Pause,
		Clock,
	} from 'lucide-vue-next';
	import { format, formatDistanceToNowStrict, isToday, isYesterday } from 'date-fns';
	import { mapState } from 'vuex';
	import { getWorkspaceMembers } from '@/actions/tmgr/workspaces';
	import { generateTaskUrl } from '@/utils/url';
	import {
		startTaskTimeCounter,
		stopTaskTimeCounter,
		updateTaskStatus,
		updateTaskPartially,
		deleteTask,
	} from '@/actions/tmgr/tasks';
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import TaskTimeInfo from '@/components/tasks/TaskTimeInfo.vue';

	export default {
		mixins: [TimePreparationMixin, TasksListMixin, SetTooltipData],
		setup() {
			const { isFeatureEnabled } = useFeatureToggles();
			return { isFeatureEnabled };
		},
		components: {
			TaskTimeInfo,
			ClockPlus,
			FilePenLine,
			Siren,
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
			GripVertical,
			Play,
			Pause,
			Clock,
		},
		emits: [
			'timer-started',
			'timer-stopped',
			'move-to-top',
			'move-to-bottom',
			'task-deleted',
			'task-archived',
		],
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
				workspaces: (state) => state.workspaces || [],
				currentWorkspaceId: (state) => {
					const setting = state.user?.settings?.find(
						(setting) => setting.key === 'current_workspace',
					);
					return setting ? setting.value : null;
				},
			}),
			currentWorkspace() {
				return this.workspaces.find(
					(workspace) => workspace.id === this.currentWorkspaceId,
				);
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
			overtime() {
				const approximatelyTime = this.getApproximatelyTime();
				if (!approximatelyTime || approximatelyTime <= 0) {
					return null;
				}
				const currentTime = this.displayTask.common_time || 0;
				const overtimeSeconds = currentTime - approximatelyTime;
				if (overtimeSeconds <= 0) {
					return null;
				}
				const hours = Math.floor(overtimeSeconds / 3600);
				const minutes = Math.floor((overtimeSeconds % 3600) / 60);
				const parts = [];
				if (hours > 0) {
					parts.push(`${hours}h`);
				}
				if (minutes > 0) {
					parts.push(`${minutes}m`);
				}
				return parts.length > 0 ? parts.join(' ') : null;
			},
			categoryCode() {
				const cat = this.task.category;
				if (cat && typeof cat === 'object' && cat.code) {
					return String(cat.code).toLowerCase();
				}
				return null;
			},
			checklistProgress() {
				const items = this.task.checkpoints;
				if (!Array.isArray(items) || items.length === 0) {
					return null;
				}
				const total = items.length;
				const checked = items.filter((c) => c && c.checked).length;
				return {
					checked,
					total,
					percent: Math.round((checked / total) * 100),
				};
			},
			formattedDisplayTime() {
				const seconds = this.displayTask.common_time || 0;
				if (seconds <= 0) return '0m';
				const hours = Math.floor(seconds / 3600);
				const minutes = Math.floor((seconds % 3600) / 60);
				if (hours > 0 && minutes > 0) return `${hours}h ${minutes}m`;
				if (hours > 0) return `${hours}h`;
				return `${minutes}m`;
			},
			formattedDate() {
				const dateStr = this.task.updated_at || this.task.created_at;
				if (!dateStr) return null;
				try {
					const d = new Date(dateStr);
					if (isToday(d)) return 'Today';
					if (isYesterday(d)) return 'Yesterday';
					return format(d, 'MMM d');
				} catch (e) {
					return null;
				}
			},
		},
		watch: {
			'task.start_time': {
				handler(newVal, oldVal) {
					if (newVal && !this.timerInterval) {
						this.startTimer();
					} else if (!newVal) {
						this.stopTimer();
					}
				},
				immediate: true,
			},
			'task.common_time': {
				handler(newVal) {
					if (!this.task.start_time) {
						this.currentDisplayTime = newVal || 0;
					}
				},
				immediate: true,
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
					const workspaceId =
						this.currentWorkspaceId ||
						this.$store.state.user?.settings?.find(
							(s) => s.key === 'current_workspace',
						)?.value;
					if (workspaceId) {
						this.workspaceMembers = await getWorkspaceMembers(workspaceId);
					}
				} catch (e) {
					console.error('Failed to load workspace members:', e);
				}
			},
			isAssigned(memberId) {
				return this.task.assignees?.some((a) => a.id === memberId) || false;
			},
			async toggleAssignee(memberId) {
				const currentAssigneeIds = this.task.assignees?.map((a) => a.id) || [];
				let newAssigneeIds;

				if (currentAssigneeIds.includes(memberId)) {
					newAssigneeIds = currentAssigneeIds.filter((id) => id !== memberId);
				} else {
					newAssigneeIds = [...currentAssigneeIds, memberId];
				}

				try {
					await updateTaskPartially(this.task.id, {
						assignees: newAssigneeIds,
					});
					const member = this.workspaceMembers.find((m) => m.id === memberId);
					if (currentAssigneeIds.includes(memberId)) {
						this.task.assignees = this.task.assignees.filter(
							(a) => a.id !== memberId,
						);
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
					task.category && typeof task.category === 'object'
						? task.category
						: null,
				);
			},
			getApproximatelyTime() {
				if (this.task.approximately_time) {
					return parseInt(this.task.approximately_time, 10);
				}
				const setting = this.task.settings?.find(
					(s) => s.key === 'approximately_time',
				);
				if (setting) {
					return parseInt(setting.value || setting.pivot?.value, 10);
				}
				return 0;
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
					hour12: false,
				});
			},
		async handleStartTimer() {
			if (!this.task.id || this.isLoadingTimer) return;

			this.isLoadingTimer = true;
			try {
				const updatedTask = await startTaskTimeCounter(this.task.id);
				Object.assign(this.task, updatedTask);
				this.$store.commit('updateSingleTask', updatedTask);
				this.$emit('timer-started', updatedTask);

				const taskStatus = this.statuses.find(
					(s) => s.id === this.task.status_id,
				);
				if (taskStatus && taskStatus.type === 'default') {
					const activeStatus = this.statuses.find((s) => s.type === 'active');
					if (activeStatus) {
						const shouldSwitch = confirm(
							`Task "${this.task.title}" is in backlog. Switch to "${activeStatus.name}" status?`,
						);
						if (shouldSwitch) {
							await updateTaskStatus(this.task.id, activeStatus.id);
							this.task.status_id = activeStatus.id;
							this.$store.commit('updateSingleTask', this.task);
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
				this.$store.commit('updateSingleTask', updatedTask);
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
				if (
					!confirm(`Are you sure you want to archive "${this.task.title}"?`)
				) {
					return;
				}

				try {
					const archiveStatus = this.statuses.find(
						(s) => s.type === 'archived',
					);
					if (!archiveStatus) {
						alert(
							'Archive status not found. Please create an archive status first.',
						);
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
