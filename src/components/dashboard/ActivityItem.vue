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
		<div class="flex items-start space-x-3">
			<!-- Activity Icon -->
			<div :class="iconClasses">
				<component :is="activityIcon" class="h-5 w-5" />
			</div>

			<!-- Activity Content -->
			<div class="min-w-0 flex-1">
				<div class="flex items-start justify-between gap-3">
					<div class="min-w-0 flex-1 break-words">
						<!-- Activity Title (Action) -->
						<h3 class="text-sm font-semibold leading-tight text-ink">
							{{ title }}
						</h3>

						<!-- Activity Subject (Resource name) -->
						<p v-if="subject" class="mt-0.5 text-sm text-ink-muted">
							{{ subject }}
						</p>
					</div>

					<!-- Timestamp -->
					<time
						:datetime="activity.created_at"
						class="flex-shrink-0 whitespace-nowrap text-xs text-ink-subtle"
						:title="formattedDate"
					>
						{{ time }}
					</time>
				</div>

				<!-- User Info -->
				<div class="mt-2 flex items-center space-x-2">
					<AuthorBadge
						v-if="activity.actor && activity.actor.kind !== 'user'"
						:author="activity.actor"
						:size="20"
					/>
					<template v-else>
						<MemberAvatarLink
							:user-id="activity.user.id"
							:label="activity.user.name"
						>
							<UserAvatar
								:user-id="activity.user.id"
								:name="activity.user.name"
								:has-avatar="activity.user.has_avatar ?? false"
								:size="20"
							/>
						</MemberAvatarLink>
						<span class="min-w-0 truncate text-xs text-ink-muted">
							by {{ activity.user.name }}
						</span>
					</template>
				</div>
			</div>
		</div>
	</div>
</template>

<script setup lang="ts">
	import AuthorBadge from '@/components/general/AuthorBadge.vue';
	import MemberAvatarLink from '@/components/general/MemberAvatarLink.vue';
	import UserAvatar from '@/components/general/UserAvatar.vue';
	import type { Activity } from '@/types/dashboard';
	import { cn } from '@/utils';
	import {
		activitySubject,
		activityTime,
		activityTitle,
	} from '@/utils/activityLines';
	import {
		ArrowPathIcon,
		ArrowsPointingOutIcon,
		ArrowUturnLeftIcon,
		ChatBubbleLeftIcon,
		CheckIcon,
		CodeBracketIcon,
		DocumentIcon,
		FolderIcon,
		LinkIcon,
		LinkSlashIcon,
		PencilIcon,
		PlayIcon,
		PlusIcon,
		StopIcon,
		TrashIcon,
		UserMinusIcon,
		UserPlusIcon,
		XMarkIcon,
	} from '@heroicons/vue/24/outline';
	import { computed } from 'vue';

	interface Props {
		activity: Activity;
	}

	const props = defineProps<Props>();

	const emit = defineEmits<{
		click: [activity: Activity];
	}>();

	// TM-228: the feed used to render whatever the API sent, so a row with no title or subject name
	// showed as a blank line next to a timestamp.
	const title = computed(() => activityTitle(props.activity));
	const subject = computed(() => activitySubject(props.activity));
	const time = computed(() => activityTime(props.activity));

	const formattedDate = computed(() => {
		return new Date(props.activity.created_at).toLocaleString();
	});

	const activityIcon = computed(() => {
		const iconMap: Record<string, any> = {
			task_created: PlusIcon,
			task_updated: PencilIcon,
			task_completed: CheckIcon,
			task_deleted: TrashIcon,
			task_restored: ArrowUturnLeftIcon,
			task_status_changed: ArrowPathIcon,
			task_assigned: UserPlusIcon,
			task_persona_assigned: UserPlusIcon,
			task_persona_unassigned: UserMinusIcon,
			task_timer_started: PlayIcon,
			task_timer_stopped: StopIcon,
			comment_created: ChatBubbleLeftIcon,
			comment_updated: PencilIcon,
			comment_deleted: TrashIcon,
			category_created: FolderIcon,
			category_updated: PencilIcon,
			category_deleted: TrashIcon,
			file_uploaded: DocumentIcon,
			file_deleted: TrashIcon,
			member_joined: UserPlusIcon,
			member_left: UserMinusIcon,
			routine_completed: CheckIcon,
			git_commit_pushed: CodeBracketIcon,
			git_branch_created: ArrowsPointingOutIcon,
			git_branch_deleted: TrashIcon,
			git_pr_opened: ArrowPathIcon,
			git_pr_closed: XMarkIcon,
			git_pr_merged: CheckIcon,
			git_repo_connected: LinkIcon,
			git_repo_disconnected: LinkSlashIcon,
			github_app_installed: LinkIcon,
			github_app_uninstalled: LinkSlashIcon,
			cursor_agent_started: PlayIcon,
			cursor_agent_finished: CheckIcon,
			cursor_agent_failed: XMarkIcon,
			cursor_agent_stopped: StopIcon,
		};

		return iconMap[props.activity.type] || PlusIcon;
	});

	const iconClasses = computed(() => {
		const colorMap: Record<string, string> = {
			task_created: 'bg-status-done-bg text-status-done-fg',
			task_updated: 'bg-status-todo-bg text-status-todo-fg',
			task_completed: 'bg-status-done-bg text-status-done-fg',
			task_deleted: 'bg-status-fix-bg text-status-fix-fg',
			task_restored: 'bg-status-todo-bg text-status-todo-fg',
			task_status_changed: 'bg-status-progress-bg text-status-progress-fg',
			task_assigned:
				'bg-purple-100 text-purple-600 dark:bg-purple-900/20 dark:text-purple-400',
			task_persona_assigned:
				'bg-violet-100 text-violet-600 dark:bg-violet-900/20 dark:text-violet-400',
			task_persona_unassigned:
				'bg-violet-100 text-violet-600 dark:bg-violet-900/20 dark:text-violet-400',
			task_timer_started: 'bg-status-done-bg text-status-done-fg',
			task_timer_stopped: 'bg-status-fix-bg text-status-fix-fg',
			comment_created: 'bg-status-todo-bg text-status-todo-fg',
			comment_updated: 'bg-status-progress-bg text-status-progress-fg',
			comment_deleted: 'bg-status-fix-bg text-status-fix-fg',
			category_created:
				'bg-indigo-100 text-indigo-600 dark:bg-indigo-900/20 dark:text-indigo-400',
			category_updated: 'bg-status-progress-bg text-status-progress-fg',
			category_deleted: 'bg-status-fix-bg text-status-fix-fg',
			file_uploaded: 'bg-status-done-bg text-status-done-fg',
			file_deleted: 'bg-status-fix-bg text-status-fix-fg',
			member_joined: 'bg-status-done-bg text-status-done-fg',
			member_left: 'bg-status-fix-bg text-status-fix-fg',
			routine_completed: 'bg-status-done-bg text-status-done-fg',
			git_commit_pushed:
				'bg-violet-100 text-violet-600 dark:bg-violet-900/20 dark:text-violet-400',
			git_branch_created:
				'bg-violet-100 text-violet-600 dark:bg-violet-900/20 dark:text-violet-400',
			git_branch_deleted:
				'bg-violet-100 text-violet-600 dark:bg-violet-900/20 dark:text-violet-400',
			git_pr_opened:
				'bg-violet-100 text-violet-600 dark:bg-violet-900/20 dark:text-violet-400',
			git_pr_closed:
				'bg-violet-100 text-violet-600 dark:bg-violet-900/20 dark:text-violet-400',
			git_pr_merged:
				'bg-violet-100 text-violet-600 dark:bg-violet-900/20 dark:text-violet-400',
			git_repo_connected:
				'bg-violet-100 text-violet-600 dark:bg-violet-900/20 dark:text-violet-400',
			git_repo_disconnected:
				'bg-violet-100 text-violet-600 dark:bg-violet-900/20 dark:text-violet-400',
			github_app_installed:
				'bg-violet-100 text-violet-600 dark:bg-violet-900/20 dark:text-violet-400',
			github_app_uninstalled:
				'bg-violet-100 text-violet-600 dark:bg-violet-900/20 dark:text-violet-400',
			cursor_agent_started:
				'bg-purple-100 text-purple-600 dark:bg-purple-900/20 dark:text-purple-400',
			cursor_agent_finished:
				'bg-purple-100 text-purple-600 dark:bg-purple-900/20 dark:text-purple-400',
			cursor_agent_failed:
				'bg-purple-100 text-purple-600 dark:bg-purple-900/20 dark:text-purple-400',
			cursor_agent_stopped:
				'bg-purple-100 text-purple-600 dark:bg-purple-900/20 dark:text-purple-400',
		};

		const baseClasses =
			'flex items-center justify-center w-10 h-10 rounded-full flex-shrink-0';
		const colorClass =
			colorMap[props.activity.type] || 'bg-surface-sunken text-ink-muted';

		return cn(baseClasses, colorClass);
	});

	const itemClasses = computed(() => {
		return cn(
			'p-4 rounded-lg transition-colors duration-150',
			'hover:bg-surface-hover',
			'cursor-pointer',
		);
	});

	const accessibilityLabel = computed(() => {
		return `${props.activity.user.name} ${title.value} ${subject.value} ${time.value}`.trim();
	});

	const handleClick = () => {
		emit('click', props.activity);
	};
</script>

<style scoped>
	/* Additional custom styles if needed */
</style>
