<template>
	<div
		:class="itemClasses"
		:title="rowTitle"
		@click="handleClick"
		@keydown.enter="handleClick"
		@keydown.space.prevent="handleClick"
		tabindex="0"
		role="button"
		:aria-label="accessibilityLabel"
	>
		<div class="flex min-w-0 items-center gap-3">
			<div class="relative shrink-0">
				<MemberAvatarLink :user-id="member.id" :label="member.name">
					<UserAvatar
						:user-id="member.id"
						:name="member.name"
						:has-avatar="member.has_avatar ?? false"
						:size="40"
					/>
				</MemberAvatarLink>
				<div
					:class="statusIndicatorClasses"
					:title="
						member.is_online ? 'Active in the last 5 minutes' : 'Inactive'
					"
				></div>
			</div>

			<div class="min-w-0 flex-1">
				<div class="flex min-w-0 items-center gap-2">
					<h4 class="min-w-0 truncate text-sm font-medium text-ink">
						{{ member.name }}
					</h4>
					<span
						class="shrink-0 rounded bg-surface-sunken px-1.5 py-0.5 text-2xs tracking-wide text-ink-subtle uppercase"
						>{{ member.role }}</span
					>
				</div>
				<div
					class="mt-0.5 flex min-w-0 items-center gap-1 text-xs text-ink-subtle"
				>
					<template v-if="member.current_task">
						<PlayIcon class="h-3 w-3 shrink-0 text-status-done" />
						<button
							@click.stop="handleTaskClick(member.current_task.id)"
							class="min-w-0 truncate hover:text-brand"
							:title="member.current_task.title"
						>
							{{ member.current_task.title }}
						</button>
						<span
							v-if="timerDuration && showSurface('timer.member')"
							class="shrink-0 font-mono text-status-done-fg"
							>· {{ timerDuration }}</span
						>
					</template>
					<template v-else-if="member.last_task">
						<ClockIcon class="h-3 w-3 shrink-0" />
						<span class="shrink-0">Last touched</span>
						<button
							@click.stop="handleTaskClick(member.last_task.id)"
							class="min-w-0 truncate hover:text-brand"
							:title="member.last_task.title"
						>
							{{ member.last_task.title }}
						</button>
					</template>
					<template v-else-if="!member.last_activity_at"
						><span>No recent activity</span></template
					>
				</div>
			</div>
		</div>

		<p
			class="mt-2 truncate text-xs text-ink-subtle tabular-nums sm:hidden"
			aria-hidden="true"
		>
			{{ member.done_count }} done · {{ tracked }} ·
			{{ member.active_tasks }} active · {{ lastActivityHuman || '—' }}
		</p>

		<div
			class="hidden h-6 w-16 items-end justify-end gap-px md:flex"
			:title="`Activity, last ${dailyActivityPoints.length} day(s)`"
			aria-hidden="true"
		>
			<span
				v-for="(h, i) in bars"
				:key="dailyActivityPoints[i]?.date ?? i"
				class="max-w-1.5 min-w-px flex-1 rounded-sm"
				:class="h > 0 ? 'bg-brand' : 'bg-surface-sunken'"
				:style="{ height: `${Math.max(h, 2)}px` }"
			></span>
		</div>

		<dl class="hidden text-sm sm:contents">
			<div class="text-right text-ink tabular-nums">
				<dt class="sr-only">Done</dt>
				<dd>{{ member.done_count }}</dd>
			</div>
			<div class="text-right text-ink tabular-nums">
				<dt class="sr-only">Tracked</dt>
				<dd>{{ tracked }}</dd>
			</div>
			<div class="text-right text-ink tabular-nums">
				<dt class="sr-only">Active tasks</dt>
				<dd>{{ member.active_tasks }}</dd>
			</div>
			<div class="flex min-w-0 items-center justify-end gap-1.5 text-xs">
				<dt class="sr-only">Last active</dt>
				<dd class="flex min-w-0 items-center gap-1.5 text-ink-muted">
					<time
						v-if="member.last_activity_at"
						class="truncate"
						:datetime="member.last_activity_at"
						:title="formattedLastActivity"
						>{{ lastActivityHuman }}</time
					>
					<span v-else>—</span>
					<span
						class="h-1.5 w-1.5 shrink-0 rounded-full"
						:class="member.is_online ? 'bg-status-done' : 'bg-ink-faint'"
					></span>
				</dd>
			</div>
		</dl>
	</div>
</template>

<script setup lang="ts">
	import MemberAvatarLink from '@/components/general/MemberAvatarLink.vue';
	import UserAvatar from '@/components/general/UserAvatar.vue';
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import type { TeamMemberStatus } from '@/types/dashboard';
	import { cn } from '@/utils';
	import {
		formatTrackedSeconds,
		sparklineBars,
	} from '@/utils/dashboard/teamActivityFormat';
	import { ClockIcon, PlayIcon } from '@heroicons/vue/24/outline';
	import { computed, onMounted, onUnmounted, ref, watch } from 'vue';

	interface Props {
		member: TeamMemberStatus;
	}

	const props = defineProps<Props>();

	const emit = defineEmits<{
		click: [member: TeamMemberStatus];
		'task-click': [taskId: number];
	}>();

	const timerInterval = ref<NodeJS.Timeout | null>(null);
	const { showSurface } = useFeatureToggles();
	const timerDuration = ref<string>('');

	const formattedLastActivity = computed(() => {
		if (!props.member.last_activity_at) return '';
		return new Date(props.member.last_activity_at).toLocaleString();
	});

	const lastActivityHuman = computed(() => {
		if (!props.member.last_activity_at) return '';

		const now = new Date();
		const lastActivity = new Date(props.member.last_activity_at);
		const diffInMinutes = Math.floor(
			(now.getTime() - lastActivity.getTime()) / (1000 * 60),
		);

		if (diffInMinutes < 1) return 'just now';
		if (diffInMinutes < 60) return `${diffInMinutes}m ago`;

		const diffInHours = Math.floor(diffInMinutes / 60);
		if (diffInHours < 24) return `${diffInHours}h ago`;

		const diffInDays = Math.floor(diffInHours / 24);
		return `${diffInDays}d ago`;
	});

	const statusIndicatorClasses = computed(() => {
		return cn(
			'absolute -bottom-0.5 -right-0.5 w-3 h-3 rounded-full border-2 border-surface',
			props.member.is_online ? 'bg-status-done' : 'bg-ink-faint',
		);
	});

	const itemClasses = computed(() => {
		return cn(
			'min-w-0 rounded-card px-2 py-3',
			'sm:grid sm:grid-cols-[minmax(0,1fr)_3rem_4.5rem_3rem_5.5rem] sm:items-center sm:gap-x-3',
			'md:grid-cols-[minmax(0,1fr)_4rem_3rem_4.5rem_3rem_5.5rem]',
			'hover:bg-surface-hover transition-colors duration-150',
			'cursor-pointer',
		);
	});

	const tracked = computed(() =>
		formatTrackedSeconds(props.member.tracked_seconds),
	);
	const dailyActivityPoints = computed(() => props.member.daily_activity ?? []);
	const bars = computed(() => sparklineBars(dailyActivityPoints.value, 24, 2));

	const rowTitle = computed(() => {
		const parts: string[] = [];
		if (showSurface('comments.count')) {
			parts.push(`${props.member.comments_count} comments`);
		}
		parts.push(`streak ${props.member.streak}`);
		return parts.join(' · ');
	});

	const accessibilityLabel = computed(() => {
		const parts = [
			`Team member: ${props.member.name}`,
			props.member.is_online ? 'Online' : 'Offline',
		];

		if (props.member.current_task) {
			parts.push(`Working on: ${props.member.current_task.title}`);
			if (props.member.current_task.timer_running) {
				parts.push('Timer running');
			}
		} else if (props.member.last_activity_at) {
			parts.push(`Last active: ${lastActivityHuman.value}`);
		}

		parts.push(
			`${tracked.value} tracked, ${props.member.done_count} done, ${props.member.active_tasks} active, ${props.member.comments_count} comments, streak ${props.member.streak}`,
		);

		return parts.join(', ');
	});

	const updateTimerDuration = () => {
		if (
			!props.member.current_task?.timer_running ||
			!props.member.current_task?.timer_started_at
		) {
			timerDuration.value = '';
			return;
		}

		const startTime = new Date(
			props.member.current_task.timer_started_at,
		).getTime();
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
		emit('click', props.member);
	};

	const handleTaskClick = (taskId: number) => {
		emit('task-click', taskId);
	};

	// Update timer duration every second if timer is running
	onMounted(() => {
		if (props.member.current_task?.timer_running) {
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
		() => props.member.current_task?.timer_running,
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
