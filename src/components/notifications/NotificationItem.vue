<template>
	<div
		class="notification-item group flex cursor-pointer items-start gap-3 border-b border-line px-4 py-3 transition-colors last:border-b-0 hover:bg-surface-hover"
		:class="{ unread: !notification.read_at }"
		role="link"
		tabindex="0"
		@click="$emit('click')"
		@keydown.enter.self="$emit('click')"
		@keydown.space.self.prevent="$emit('click')"
	>
		<div
			class="flex h-10 w-10 shrink-0 items-center justify-center rounded-pill"
			:class="
				notification.read_at
					? 'bg-surface-sunken text-ink-subtle'
					: 'bg-brand-bg text-brand'
			"
		>
			<component :is="icon" :size="20" />
		</div>

		<div class="min-w-0 flex-1">
			<div class="mb-1 flex items-start gap-2">
				<div
					class="min-w-0 text-sm [overflow-wrap:anywhere] text-ink"
					:class="notification.read_at ? 'font-medium' : 'font-semibold'"
				>
					{{ title }}
				</div>
				<span
					v-if="!notification.read_at"
					class="mt-1.5 h-2 w-2 shrink-0 rounded-pill bg-brand"
					aria-hidden="true"
				></span>
			</div>
			<div
				v-if="notification.message"
				class="mb-1 line-clamp-2 text-[0.8125rem] [overflow-wrap:anywhere] text-ink-muted"
			>
				{{ notification.message }}
			</div>
			<div class="text-xs text-ink-subtle">{{ formattedTime }}</div>
		</div>

		<button
			class="shrink-0 rounded-md p-1 text-ink-subtle opacity-0 transition-colors group-hover:opacity-100 hover:bg-surface-hover hover:text-status-fix-fg focus-visible:opacity-100"
			@click.stop="$emit('delete')"
			aria-label="Delete notification"
		>
			<X :size="16" />
		</button>
	</div>
</template>

<script>
	import {
		normalizeNotificationType,
		notificationTitle,
	} from '@/utils/notificationTypes';
	import { formatDistanceToNow } from 'date-fns';
	import {
		AlertCircle,
		CheckCircle,
		Edit,
		FileText,
		Folder,
		MessageCircle,
		Trash2,
		UserMinus,
		UserPlus,
		X,
	} from 'lucide-vue-next';
	import { computed, defineComponent } from 'vue';

	export default defineComponent({
		name: 'NotificationItem',
		components: {
			CheckCircle,
			Edit,
			Trash2,
			MessageCircle,
			Folder,
			FileText,
			UserPlus,
			UserMinus,
			AlertCircle,
			X,
		},
		props: {
			notification: {
				type: Object,
				required: true,
			},
		},
		emits: ['click', 'delete'],
		setup(props) {
			const icon = computed(() => {
				const type = normalizeNotificationType(props.notification.type);

				const iconMap = {
					task_created: FileText,
					task_updated: Edit,
					task_status_changed: CheckCircle,
					task_assigned: UserPlus,
					task_persona_assigned: UserPlus,
					task_deleted: Trash2,
					task_restored: CheckCircle,
					task_completed: CheckCircle,
					comment_created: MessageCircle,
					comment_updated: MessageCircle,
					comment_deleted: MessageCircle,
					category_created: Folder,
					category_updated: Folder,
					category_deleted: Trash2,
					category_restored: Folder,
					file_uploaded: FileText,
					file_deleted: Trash2,
					member_joined: UserPlus,
					member_left: UserMinus,
					page_created: FileText,
					page_updated: Edit,
					page_mentioned: MessageCircle,
				};

				return iconMap[type] || AlertCircle;
			});

			const title = computed(() => notificationTitle(props.notification));

			const formattedTime = computed(() => {
				try {
					return formatDistanceToNow(new Date(props.notification.created_at), {
						addSuffix: true,
					});
				} catch (error) {
					return props.notification.created_at;
				}
			});

			return {
				icon,
				title,
				formattedTime,
			};
		},
	});
</script>
