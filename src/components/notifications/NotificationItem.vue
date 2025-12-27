<template>
	<div
		class="notification-item"
		:class="{ unread: !notification.read_at }"
		@click="$emit('click')"
	>
		<div class="notification-icon">
			<component :is="icon" :size="20" />
		</div>

		<div class="notification-content">
			<div class="notification-title">{{ notification.title }}</div>
			<div v-if="notification.message" class="notification-message">
				{{ notification.message }}
			</div>
			<div class="notification-time">{{ formattedTime }}</div>
		</div>

		<button
			class="notification-delete"
			@click.stop="$emit('delete')"
			aria-label="Delete notification"
		>
			<X :size="16" />
		</button>
	</div>
</template>

<script>
import { defineComponent, computed } from 'vue';
import {
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
} from 'lucide-vue-next';
import { formatDistanceToNow } from 'date-fns';

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
			const type = props.notification.type;

			const iconMap = {
				task_created: FileText,
				task_updated: Edit,
				task_status_changed: CheckCircle,
				task_assigned: UserPlus,
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
			};

			return iconMap[type] || AlertCircle;
		});

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
			formattedTime,
		};
	},
});
</script>

<style lang="scss" scoped>
.notification-item {
	display: flex;
	align-items: flex-start;
	gap: 0.75rem;
	padding: 0.75rem 1rem;
	cursor: pointer;
	transition: background-color 0.2s;
	border-bottom: 1px solid rgba(0, 0, 0, 0.05);

	&:last-child {
		border-bottom: none;
	}

	&:hover {
		background-color: rgba(0, 0, 0, 0.02);
	}

	&.unread {
		background-color: rgba(59, 130, 246, 0.05);

		&:hover {
			background-color: rgba(59, 130, 246, 0.1);
		}
	}

	.dark & {
		border-bottom-color: rgba(255, 255, 255, 0.05);

		&:hover {
			background-color: rgba(255, 255, 255, 0.05);
		}

		&.unread {
			background-color: rgba(59, 130, 246, 0.1);

			&:hover {
				background-color: rgba(59, 130, 246, 0.15);
			}
		}
	}
}

.notification-icon {
	flex-shrink: 0;
	width: 40px;
	height: 40px;
	display: flex;
	align-items: center;
	justify-content: center;
	background-color: rgba(59, 130, 246, 0.1);
	color: var(--tmgr-blue);
	border-radius: 50%;
}

.notification-content {
	flex: 1;
	min-width: 0;
}

.notification-title {
	font-size: 0.875rem;
	font-weight: 500;
	margin-bottom: 0.25rem;
	color: inherit;
}

.notification-message {
	font-size: 0.8125rem;
	color: rgba(0, 0, 0, 0.6);
	margin-bottom: 0.25rem;
	overflow: hidden;
	text-overflow: ellipsis;
	display: -webkit-box;
	-webkit-line-clamp: 2;
	-webkit-box-orient: vertical;

	.dark & {
		color: rgba(255, 255, 255, 0.6);
	}
}

.notification-time {
	font-size: 0.75rem;
	color: rgba(0, 0, 0, 0.5);

	.dark & {
		color: rgba(255, 255, 255, 0.5);
	}
}

.notification-delete {
	flex-shrink: 0;
	padding: 0.25rem;
	background: transparent;
	border: none;
	color: rgba(0, 0, 0, 0.4);
	cursor: pointer;
	border-radius: 0.25rem;
	transition: all 0.2s;
	opacity: 0;

	.notification-item:hover & {
		opacity: 1;
	}

	&:hover {
		background-color: rgba(239, 68, 68, 0.1);
		color: #ef4444;
	}

	.dark & {
		color: rgba(255, 255, 255, 0.4);

		&:hover {
			background-color: rgba(239, 68, 68, 0.2);
		}
	}
}
</style>

