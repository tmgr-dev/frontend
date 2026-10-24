<template>
	<div
		:class="itemClasses"
		@click="handleClick"
		@keydown.enter="handleClick"
		@keydown.space.prevent="handleClick"
		tabindex="0"
		role="button"
		:aria-label="accessibilityLabel"
	>
		<div class="flex items-start justify-between">
			<div class="min-w-0 flex-1">
				<!-- Task Title and Status -->
				<div class="space-x-2 mb-2 flex items-center">
					<div :class="statusIndicatorClasses" :title="statusText"></div>
					<h4 class="truncate text-sm font-medium text-ink">
						{{ task.title }}
					</h4>
					<div v-if="task.is_overdue" class="flex items-center">
						<ExclamationTriangleIcon class="h-4 w-4 text-status-fix" />
						<span class="sr-only">Overdue</span>
					</div>
				</div>

				<!-- Task Metadata -->
				<div class="space-x-4 flex items-center text-xs text-ink-subtle">
					<!-- Category -->
					<div
						v-if="task.category && showSurface('categories.badge')"
						class="space-x-1 flex items-center"
					>
						<div
							class="h-2 w-2 rounded-full"
							:style="{ backgroundColor: task.category.color || '#6B7280' }"
						></div>
						<span>{{ task.category.name }}</span>
					</div>

					<!-- Assignee -->
					<div class="space-x-1 flex items-center">
						<UserIcon class="h-3 w-3" />
						<span>{{ task.user.name }}</span>
					</div>

					<!-- Priority -->
					<div v-if="task.priority" class="space-x-1 flex items-center">
						<component :is="priorityIcon" :class="priorityIconClasses" />
						<span class="capitalize">{{ task.priority }}</span>
					</div>

					<!-- Last Updated -->
					<div class="space-x-1 flex items-center">
						<ClockIcon class="h-3 w-3" />
						<time :datetime="task.updated_at" :title="formattedDate">
							{{ task.updated_at_human }}
						</time>
					</div>
				</div>

				<!-- Assignees (if multiple) -->
				<div
					v-if="task.assignees && task.assignees.length > 1"
					class="space-x-1 mt-2 flex items-center"
				>
					<span class="text-xs text-ink-subtle">Assignees:</span>
					<div class="-space-x-1 flex">
						<MemberAvatarLink
							v-for="assignee in task.assignees.slice(0, 3)"
							:key="assignee.id"
							:user-id="assignee.id"
							:label="assignee.name"
						>
							<UserAvatar
								:user-id="assignee.id"
								:name="assignee.name"
								:has-avatar="assignee.has_avatar ?? false"
								:size="20"
								class="border border-surface"
							/>
						</MemberAvatarLink>
						<div
							v-if="task.assignees.length > 3"
							class="flex h-5 w-5 items-center justify-center rounded-full border border-surface bg-surface-sunken"
							:title="`+${task.assignees.length - 3} more`"
						>
							<span class="text-xs font-medium text-ink-muted">
								+{{ task.assignees.length - 3 }}
							</span>
						</div>
					</div>
				</div>
			</div>

			<!-- Actions -->
			<div class="space-x-2 ml-4 flex items-center">
				<!-- Timer Button -->
				<Button
					v-if="isFeatureEnabled('task.countdown')"
					variant="ghost"
					size="sm"
					@click.stop="handleTimerToggle"
					:class="timerButtonClasses"
					:title="task.timer_running ? 'Stop timer' : 'Start timer'"
				>
					<component :is="timerIcon" class="h-4 w-4" />
					<span v-if="task.timer_running && timerDuration" class="ml-1 text-xs">
						{{ timerDuration }}
					</span>
				</Button>

				<DoneCheckbox
					v-if="simpleMode"
					class="text-xs"
					:model-value="isDone(task.status)"
					:disabled="doneStatusId === null"
					:tooltip="DONE_DISABLED_TOOLTIP"
					@update:model-value="handleDoneChange"
				/>

				<!-- Status Dropdown -->
				<Select
					v-else-if="task.status"
					:model-value="task.status.id.toString()"
					@update:model-value="handleStatusChange"
				>
					<SelectTrigger class="h-8 w-24 text-xs">
						<SelectValue />
					</SelectTrigger>
					<SelectContent>
						<SelectItem
							v-for="status in availableStatuses"
							:key="status.id"
							:value="status.id.toString()"
						>
							<div class="space-x-2 flex items-center">
								<div
									class="h-2 w-2 rounded-full"
									:style="{ backgroundColor: status.color || '#6B7280' }"
								></div>
								<span>{{ status.name }}</span>
							</div>
						</SelectItem>
					</SelectContent>
				</Select>

				<!-- Fallback when no status -->
				<div
					v-else
					class="flex h-8 w-24 items-center justify-center rounded-md border border-line text-xs text-ink-subtle"
				>
					No Status
				</div>
			</div>
		</div>
	</div>
</template>

<script setup lang="ts">
	import MemberAvatarLink from '@/components/general/MemberAvatarLink.vue';
	import UserAvatar from '@/components/general/UserAvatar.vue';
	import DoneCheckbox from '@/components/tasks/DoneCheckbox.vue';
	import Button from '@/components/ui/button/Button.vue';
	import {
		Select,
		SelectContent,
		SelectItem,
		SelectTrigger,
		SelectValue,
	} from '@/components/ui/select';
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import { useWorkflowMode } from '@/composable/useWorkflowMode';
	import type { RecentTask } from '@/types/dashboard';
	import { cn } from '@/utils';
	import { priorityOption } from '@/utils/priority';
	import {
		DONE_DISABLED_TOOLTIP,
		isDone,
		stateLabel,
	} from '@/utils/simpleStatus';
	import {
		ClockIcon,
		ExclamationTriangleIcon,
		PlayIcon,
		StopIcon,
		UserIcon,
	} from '@heroicons/vue/24/outline';
	import { computed, onMounted, onUnmounted, ref, watch } from 'vue';

	interface Props {
		task: RecentTask;
		availableStatuses?: Array<{
			id: number;
			name: string;
			color?: string;
			type: string;
		}>;
	}

	const props = withDefaults(defineProps<Props>(), {
		availableStatuses: () => [
			{ id: 1, name: 'To Do', color: '#6B7280', type: 'pending' },
			{ id: 2, name: 'In Progress', color: '#3B82F6', type: 'active' },
			{ id: 3, name: 'Done', color: '#10B981', type: 'archived' },
		],
	});

	const emit = defineEmits<{
		click: [task: RecentTask];
		'timer-toggle': [task: RecentTask];
		'status-change': [statusId: number];
	}>();

	const { isFeatureEnabled, showSurface } = useFeatureToggles();
	const { simpleMode, doneStatusId, openStatusId } = useWorkflowMode();
	const statusText = computed(() =>
		simpleMode.value
			? stateLabel(props.task.status)
			: props.task.status?.name || 'Unknown',
	);
	const handleDoneChange = (checked: boolean) => {
		const target = checked ? doneStatusId.value : openStatusId.value;
		if (target !== null) emit('status-change', target);
	};

	const timerInterval = ref<NodeJS.Timeout | null>(null);
	const timerDuration = ref<string>('');

	const formattedDate = computed(() => {
		return new Date(props.task.updated_at).toLocaleString();
	});

	const priorityIcon = computed(() => priorityOption(props.task.priority).icon);

	const priorityIconClasses = computed(() =>
		cn('h-3 w-3', priorityOption(props.task.priority).colorClass),
	);

	const timerIcon = computed(() => {
		return props.task.timer_running ? StopIcon : PlayIcon;
	});

	const timerButtonClasses = computed(() => {
		return cn(
			'p-1',
			props.task.timer_running
				? 'text-status-fix-fg hover:text-status-fix'
				: 'text-status-done-fg hover:text-status-done',
		);
	});

	const statusIndicatorClasses = computed(() => {
		return cn(
			'w-3 h-3 rounded-full shrink-0',
			`bg-[${props.task.status?.color || '#6B7280'}]`,
		);
	});

	const isTimeExceeded = computed(() => {
		const approximatelyTime = getApproximatelyTime();
		if (!approximatelyTime || approximatelyTime <= 0) {
			return false;
		}
		const currentTime = (props.task as any).common_time || 0;
		return currentTime > approximatelyTime;
	});

	const getApproximatelyTime = () => {
		const task = props.task as any;
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
	};

	const itemClasses = computed(() => {
		return cn(
			'p-3 rounded-lg border border-line',
			'hover:bg-surface-hover transition-colors duration-150',
			'cursor-pointer',
			props.task.is_overdue && 'border-status-fix bg-status-fix-bg',
			isTimeExceeded.value && 'border-l-4 border-l-status-fix',
		);
	});

	const accessibilityLabel = computed(() => {
		const parts = [
			`Task: ${props.task.title}`,
			`Status: ${statusText.value}`,
			`Assigned to: ${props.task.user.name}`,
			`Updated: ${props.task.updated_at_human}`,
		];

		if (props.task.is_overdue) {
			parts.push('Overdue');
		}

		if (props.task.timer_running) {
			parts.push('Timer running');
		}

		return parts.join(', ');
	});

	const updateTimerDuration = () => {
		if (!props.task.timer_running || !props.task.timer_started_at) {
			timerDuration.value = '';
			return;
		}

		const startTime = new Date(props.task.timer_started_at).getTime();
		const now = Date.now();
		const duration = Math.floor((now - startTime) / 1000);

		const hours = Math.floor(duration / 3600);
		const minutes = Math.floor((duration % 3600) / 60);
		const seconds = duration % 60;

		if (hours > 0) {
			timerDuration.value = `${hours}:${minutes
				.toString()
				.padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
		} else {
			timerDuration.value = `${minutes}:${seconds.toString().padStart(2, '0')}`;
		}
	};

	const handleClick = () => {
		emit('click', props.task);
	};

	const handleTimerToggle = () => {
		emit('timer-toggle', props.task);
	};

	const handleStatusChange = (statusId: string) => {
		emit('status-change', parseInt(statusId));
	};

	// Update timer duration every second if timer is running
	onMounted(() => {
		if (props.task.timer_running) {
			updateTimerDuration();
			timerInterval.value = setInterval(updateTimerDuration, 1000);
		}
	});

	onUnmounted(() => {
		if (timerInterval.value) {
			clearInterval(timerInterval.value);
		}
	});

	// Watch for timer state changes
	watch(
		() => props.task.timer_running,
		(isRunning) => {
			if (isRunning) {
				updateTimerDuration();
				timerInterval.value = setInterval(updateTimerDuration, 1000);
			} else {
				if (timerInterval.value) {
					clearInterval(timerInterval.value);
					timerInterval.value = null;
				}
				timerDuration.value = '';
			}
		},
	);
</script>

<style scoped>
	/* Additional custom styles if needed */
</style>
