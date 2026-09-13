<template>
	<!-- CHANGES: Added min-h-64 to prevent CLS -->
	<div class="team-activity-widget min-h-64">
		<div
			class="team-activity-header mb-4 flex flex-wrap items-center justify-between gap-2"
		>
			<h3 class="text-lg font-semibold text-ink">Team Activity</h3>
			<div class="flex flex-wrap items-center gap-2 sm:gap-3">
				<div
					class="inline-flex overflow-hidden rounded-md border border-line text-xs"
					role="tablist"
					aria-label="Activity window"
				>
					<button
						v-for="w in windows"
						:key="w.key"
						type="button"
						role="tab"
						:aria-selected="w.key === window"
						:class="[
							'px-2.5 py-1',
							w.key === window
								? 'bg-surface-sunken text-ink'
								: 'text-ink-faint hover:text-ink',
						]"
						@click="emit('window-change', w.key)"
					>
						{{ w.label }}
					</button>
				</div>
				<div class="flex items-center gap-1 text-xs text-ink-subtle">
					<div
						class="h-2 w-2 rounded-full bg-status-done"
						:class="onlineCount > 0 && 'animate-pulse'"
					></div>
					<span>{{ onlineCount }} active now</span>
				</div>
				<Button
					variant="ghost"
					size="sm"
					@click="refreshTeamActivity"
					:disabled="loading"
					class="p-2"
					aria-label="Refresh team activity"
				>
					<ArrowPathIcon :class="['h-4 w-4', loading && 'animate-spin']" />
				</Button>
			</div>
		</div>

		<div class="team-activity-content">
			<!-- Loading skeleton -->
			<template
				v-if="
					loading &&
					(!teamActivity ||
						!teamActivity.members ||
						teamActivity.members.length === 0)
				"
			>
				<div class="space-y-3">
					<TeamMemberSkeleton v-for="i in 3" :key="`skeleton-${i}`" />
				</div>
			</template>

			<!-- Team members list -->
			<template
				v-else-if="
					teamActivity &&
					teamActivity.members &&
					teamActivity.members.length > 0
				"
			>
				<div class="space-y-3">
					<TeamMemberItem
						v-for="member in teamActivity.members"
						:key="member.id"
						:member="member"
						@click="handleMemberClick(member)"
						@task-click="handleTaskClick"
					/>
				</div>

				<!-- Team summary -->
				<div class="mt-4 border-t border-line pt-4">
					<div
						class="grid grid-cols-2 gap-4 text-center sm:grid-cols-3 lg:grid-cols-5"
					>
						<div>
							<div class="text-lg font-semibold tabular-nums text-ink">
								{{ teamActivity.total_members }}
							</div>
							<div class="text-2xs uppercase tracking-wide text-ink-subtle">
								Total members
							</div>
						</div>
						<div>
							<div
								class="text-lg font-semibold tabular-nums text-status-done-fg"
							>
								{{ teamActivity.active_today }}
							</div>
							<div class="text-2xs uppercase tracking-wide text-ink-subtle">
								Active today
							</div>
						</div>
						<div>
							<div
								class="text-lg font-semibold tabular-nums text-status-progress-fg"
							>
								{{ teamActivity.active_timers }}
							</div>
							<div class="text-2xs uppercase tracking-wide text-ink-subtle">
								Active timers
							</div>
						</div>
						<div>
							<div class="text-lg font-semibold tabular-nums text-brand-fg">
								{{ formatTrackedSeconds(teamActivity.tracked_seconds) }}
							</div>
							<div class="text-2xs uppercase tracking-wide text-ink-subtle">
								Tracked · {{ windowLabel }}
							</div>
						</div>
						<div>
							<div class="text-lg font-semibold tabular-nums text-ink">
								{{ teamActivity.done_count }}
							</div>
							<div class="text-2xs uppercase tracking-wide text-ink-subtle">
								Done · {{ windowLabel }}
							</div>
						</div>
					</div>
				</div>
			</template>

			<!-- Empty state -->
			<template v-else>
				<EmptyState
					icon="users"
					title="No team members"
					description="Invite team members to see their activity here."
					:action="{
						label: 'Invite Members',
						onClick: inviteMembers,
					}"
				/>
			</template>
		</div>
	</div>
</template>

<script setup lang="ts">
	import EmptyState from '@/components/EmptyState.vue';
	import Button from '@/components/ui/button/Button.vue';
	import type {
		TeamActivityWindow,
		TeamMemberActivity,
		TeamMemberStatus,
	} from '@/types/dashboard';
	import { formatTrackedSeconds } from '@/utils/dashboard/teamActivityFormat';
	import { ArrowPathIcon } from '@heroicons/vue/24/outline';
	import { computed } from 'vue';
	import TeamMemberItem from './TeamMemberItem.vue';
	import TeamMemberSkeleton from './TeamMemberSkeleton.vue';

	interface Props {
		teamActivity: TeamMemberActivity | null;
		loading: boolean;
		window: TeamActivityWindow;
	}

	const props = defineProps<Props>();

	const emit = defineEmits<{
		refresh: [];
		'member-click': [member: TeamMemberStatus];
		'task-click': [taskId: number];
		'invite-members': [];
		'window-change': [w: TeamActivityWindow];
	}>();

	const windows: { key: TeamActivityWindow; label: string }[] = [
		{ key: 'today', label: 'Today' },
		{ key: '7d', label: '7d' },
		{ key: '30d', label: '30d' },
	];
	const windowLabel = computed(() => {
		const key = props.teamActivity?.window ?? props.window;
		return windows.find((w) => w.key === key)?.label ?? key;
	});

	const onlineCount = computed(() => {
		return props.teamActivity?.online_members || 0;
	});

	const refreshTeamActivity = () => {
		emit('refresh');
	};

	// Navigation belongs to the page: /profile/{id}, /tasks/{id} and /workspace/invite
	// are not routes, and a task opens in the modal rather than a page of its own.
	const handleMemberClick = (member: TeamMemberStatus) => {
		emit('member-click', member);
	};

	const handleTaskClick = (taskId: number) => {
		emit('task-click', taskId);
	};

	const inviteMembers = () => {
		emit('invite-members');
	};
</script>

<style scoped>
	.team-activity-widget {
		@apply rounded-card border border-line bg-surface p-6 shadow-tmgr-xs;
	}

	.team-activity-header {
		@apply mb-4 border-b border-line pb-4;
	}

	.team-activity-content {
		@apply max-h-[28rem] min-h-48 overflow-y-auto;
	}

	/* Custom scrollbar */
	.team-activity-content::-webkit-scrollbar {
		@apply w-2;
	}

	.team-activity-content::-webkit-scrollbar-track {
		@apply rounded bg-surface-sunken;
	}

	.team-activity-content::-webkit-scrollbar-thumb {
		@apply rounded bg-gray-300 hover:bg-gray-400 dark:bg-gray-600 dark:hover:bg-gray-500;
	}
</style>
