<template>
	<div class="notification-bell">
		<DropdownMenu @update:open="handleDropdownOpen">
			<DropdownMenuTrigger as-child>
				<button 
					class="notification-bell-button relative" 
					:class="{ 'has-new-notification': hasNewNotification }"
					aria-label="Notifications"
				>
					<Bell :size="20" :class="{ 'animate-bell': hasNewNotification }" />
					<span 
						v-if="unreadCount > 0" 
						class="notification-badge"
						:class="{ 'animate-bounce-gentle': hasNewNotification }"
					>
						{{ unreadCount > 99 ? '99+' : unreadCount }}
					</span>
				</button>
			</DropdownMenuTrigger>

			<DropdownMenuContent class="notification-dropdown" align="end" :side-offset="8">
				<div class="notification-header">
					<h3 class="notification-title">Notifications</h3>
					<button
						v-if="unreadCount > 0"
						@click="handleMarkAllAsRead"
						class="mark-all-read-btn"
						:disabled="loading"
					>
						Mark all as read
					</button>
				</div>

				<DropdownMenuSeparator />

				<div v-if="loading && notifications.length === 0" class="notification-loading">
					<div class="spinner"></div>
					<p>Loading...</p>
				</div>

				<div v-else-if="notifications.length === 0" class="notification-empty">
					<Bell :size="48" class="empty-icon" />
					<p>No notifications</p>
				</div>

				<div v-else class="notification-list">
					<NotificationItem
						v-for="notification in displayedNotifications"
						:key="notification.id"
						:notification="notification"
						@click="handleNotificationClick(notification)"
						@delete="handleDelete(notification.id)"
					/>
				</div>

				<DropdownMenuSeparator v-if="notifications.length > 0" />

				<div v-if="notifications.length > 0" class="notification-footer">
					<button @click="handleViewAll" class="view-all-btn">View all</button>
				</div>
			</DropdownMenuContent>
		</DropdownMenu>
	</div>
</template>

<script>
	import { defineComponent, computed, onMounted, onUnmounted } from 'vue';
	import { useRouter, useRoute } from 'vue-router';
	import { Bell } from 'lucide-vue-next';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuTrigger,
		DropdownMenuSeparator,
	} from '@/components/ui/dropdown-menu';
	import NotificationItem from './NotificationItem.vue';
	import { useNotifications } from '@/composable/useNotifications';
	import { useStore } from 'vuex';

export default defineComponent({
	name: 'NotificationBell',
	components: {
		Bell,
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuTrigger,
		DropdownMenuSeparator,
		NotificationItem,
	},
	setup() {
		const router = useRouter();
		const route = useRoute();
		const store = useStore();
		const {
			notifications,
			unreadCount,
			loading,
			hasNewNotification,
			loadNotifications,
			refreshUnreadCount,
			markNotificationAsRead,
			markAllNotificationsAsRead,
			removeNotification,
			subscribeToRealtime,
			unsubscribeFromRealtime,
			clearNewNotificationIndicator,
		} = useNotifications();

		const displayedNotifications = computed(() => {
			return notifications.value.slice(0, 10);
		});

		const getCurrentWorkspaceCode = () => {
			if (route.params.workspace_code) {
				return route.params.workspace_code;
			}
			const workspaces = store.state.workspaces || [];
			const currentWorkspaceId = store.state.user?.settings?.find(
				setting => setting.key === 'current_workspace'
			)?.value;
			const currentWorkspace = workspaces.find(
				ws => Number(ws.id) === Number(currentWorkspaceId)
			);
			return currentWorkspace?.code || 'default';
		};

		const handleNotificationClick = async (notification) => {
			try {
				if (!notification.read_at) {
					await markNotificationAsRead(notification.id);
				}

				if (notification.link) {
					let link = notification.link;
					
					// If link is a full URL, extract the path
					if (link.startsWith('http://') || link.startsWith('https://')) {
						try {
							const url = new URL(link);
							link = url.pathname;
						} catch (e) {
							console.error('Error parsing notification link:', e);
						}
					}
					
					router.push(link);
				}
			} catch (error) {
				console.error('Error handling notification click:', error);
			}
		};

		const handleMarkAllAsRead = async () => {
			try {
				await markAllNotificationsAsRead();
			} catch (error) {
				console.error('Error marking all as read:', error);
			}
		};

		const handleDelete = async (id) => {
			try {
				await removeNotification(id);
			} catch (error) {
				console.error('Error deleting notification:', error);
			}
		};

		const handleViewAll = () => {
			const workspaceCode = getCurrentWorkspaceCode();
			router.push(`/${workspaceCode}/notifications`);
		};
		
		const handleDropdownOpen = (isOpen) => {
			if (isOpen) {
				clearNewNotificationIndicator();
			}
		};

		onMounted(async () => {
			await loadNotifications();
			await refreshUnreadCount();

			const user = store.getters['user/getUser'];
			if (user?.id) {
				subscribeToRealtime(user.id);
			}
		});

		onUnmounted(() => {
			const user = store.getters['user/getUser'];
			if (user?.id) {
				unsubscribeFromRealtime(user.id);
			}
		});

		return {
			notifications,
			displayedNotifications,
			unreadCount,
			loading,
			hasNewNotification,
			handleNotificationClick,
			handleMarkAllAsRead,
			handleDelete,
			handleViewAll,
			handleDropdownOpen,
		};
	},
});
</script>

<style lang="scss" scoped>
.notification-bell {
	position: relative;
}

.notification-bell-button {
	position: relative;
	padding: 0.5rem;
	border: none;
	background: transparent;
	color: currentColor;
	cursor: pointer;
	border-radius: 0.375rem;
	transition: background-color 0.2s;

	&:hover {
		background-color: rgba(0, 0, 0, 0.05);
	}

	.dark & {
		&:hover {
			background-color: rgba(255, 255, 255, 0.1);
		}
	}
}

.notification-badge {
	position: absolute;
	top: 2px;
	right: 2px;
	min-width: 18px;
	height: 18px;
	padding: 0 4px;
	display: flex;
	align-items: center;
	justify-content: center;
	background-color: #ef4444;
	color: white;
	font-size: 10px;
	font-weight: 600;
	border-radius: 9px;
	line-height: 1;
	
	&.animate-bounce-gentle {
		animation: bounce-gentle 0.5s ease-in-out infinite;
	}
}

.has-new-notification {
	.notification-badge {
		background-color: #dc2626;
		box-shadow: 0 0 8px rgba(220, 38, 38, 0.6);
	}
}

.animate-bell {
	animation: bell-ring 0.5s ease-in-out;
	animation-iteration-count: 3;
}

@keyframes bell-ring {
	0%, 100% {
		transform: rotate(0deg);
	}
	25% {
		transform: rotate(15deg);
	}
	50% {
		transform: rotate(-15deg);
	}
	75% {
		transform: rotate(10deg);
	}
}

@keyframes bounce-gentle {
	0%, 100% {
		transform: translateY(0);
	}
	50% {
		transform: translateY(-3px);
	}
}

.notification-dropdown {
	width: 380px;
	max-height: 600px;
	padding: 0;
	overflow: hidden;
	display: flex;
	flex-direction: column;
}

.notification-header {
	display: flex;
	align-items: center;
	justify-content: space-between;
	padding: 1rem;
}

.notification-title {
	font-size: 1rem;
	font-weight: 600;
	margin: 0;
}

.mark-all-read-btn {
	padding: 0.25rem 0.5rem;
	font-size: 0.75rem;
	background: transparent;
	border: 1px solid currentColor;
	border-radius: 0.25rem;
	cursor: pointer;
	transition: all 0.2s;

	&:hover:not(:disabled) {
		background-color: rgba(0, 0, 0, 0.05);
	}

	&:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.dark & {
		&:hover:not(:disabled) {
			background-color: rgba(255, 255, 255, 0.1);
		}
	}
}

.notification-loading,
.notification-empty {
	display: flex;
	flex-direction: column;
	align-items: center;
	justify-content: center;
	padding: 2rem;
	gap: 1rem;
}

.spinner {
	width: 32px;
	height: 32px;
	border: 3px solid rgba(0, 0, 0, 0.1);
	border-top-color: currentColor;
	border-radius: 50%;
	animation: spin 0.8s linear infinite;
}

@keyframes spin {
	to {
		transform: rotate(360deg);
	}
}

.empty-icon {
	opacity: 0.3;
}

.notification-list {
	max-height: 480px;
	overflow-y: auto;
}

.notification-footer {
	padding: 0.75rem;
	text-align: center;
	border-top: 1px solid rgba(0, 0, 0, 0.1);

	.dark & {
		border-top-color: rgba(255, 255, 255, 0.1);
	}
}

.view-all-btn {
	width: 100%;
	padding: 0.5rem;
	background: transparent;
	border: none;
	color: var(--tmgr-blue);
	font-weight: 500;
	cursor: pointer;
	border-radius: 0.25rem;
	transition: background-color 0.2s;

	&:hover {
		background-color: rgba(0, 0, 0, 0.05);
	}

	.dark & {
		&:hover {
			background-color: rgba(255, 255, 255, 0.1);
		}
	}
}
</style>

