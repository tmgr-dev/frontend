<template>
	<div>
		<teleport to="title">Notifications</teleport>

		<BaseLayout>
		<template #body>
			<div class="notifications-page">
				<div class="page-header">
					<h1 class="page-title">Notifications</h1>
					<div class="header-actions">
						<button
							@click="handleGoToSettings"
							class="settings-btn"
							title="Notification Settings"
						>
							<Settings :size="20" />
							<span>Settings</span>
						</button>
						<button
							v-if="unreadCount > 0"
							@click="handleMarkAllAsRead"
							class="mark-all-btn"
							:disabled="loading"
						>
							Mark all as read
						</button>
					</div>
				</div>

				<div v-if="loading && notifications.length === 0" class="loading-state">
					<div class="spinner"></div>
					<p>Loading notifications...</p>
				</div>

				<div v-else-if="error" class="error-state">
					<p>{{ error.message }}</p>
					<button @click="handleRetry" class="retry-btn">Try again</button>
				</div>

				<div v-else-if="notifications.length === 0" class="empty-state">
					<Bell :size="64" class="empty-icon" />
					<h2>No notifications</h2>
					<p>You're all caught up!</p>
				</div>

				<div v-else class="notifications-list">
					<NotificationItem
						v-for="notification in notifications"
						:key="notification.id"
						:notification="notification"
						@click="handleNotificationClick(notification)"
						@delete="handleDelete(notification.id)"
					/>

					<div v-if="hasMore" class="load-more-section">
						<button
							@click="handleLoadMore"
							class="load-more-btn"
							:disabled="loading"
						>
							{{ loading ? 'Loading...' : 'Load more' }}
						</button>
					</div>
				</div>
			</div>
		</template>
		</BaseLayout>
	</div>
</template>

<script>
import { defineComponent, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import { Bell, Settings } from 'lucide-vue-next';
import BaseLayout from '@/components/layouts/BaseLayout.vue';
import NotificationItem from '@/components/notifications/NotificationItem.vue';
import { useNotifications } from '@/composable/useNotifications';

export default defineComponent({
	name: 'NotificationsPage',
	components: {
		BaseLayout,
		Bell,
		Settings,
		NotificationItem,
	},
	setup() {
		const router = useRouter();
		const {
			notifications,
			unreadCount,
			loading,
			error,
			hasMore,
			loadNotifications,
			loadMore,
			markNotificationAsRead,
			markAllNotificationsAsRead,
			removeNotification,
		} = useNotifications();

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
			} catch (err) {
				console.error('Error handling notification click:', err);
			}
		};

		const handleMarkAllAsRead = async () => {
			try {
				await markAllNotificationsAsRead();
			} catch (err) {
				console.error('Error marking all as read:', err);
			}
		};

		const handleDelete = async (id) => {
			try {
				await removeNotification(id);
			} catch (err) {
				console.error('Error deleting notification:', err);
			}
		};

		const handleLoadMore = async () => {
			try {
				await loadMore();
			} catch (err) {
				console.error('Error loading more notifications:', err);
			}
		};

		const handleRetry = () => {
			loadNotifications();
		};

		const handleGoToSettings = () => {
			router.push('/settings?tab=notification');
		};

		onMounted(() => {
			loadNotifications();
		});

		return {
			notifications,
			unreadCount,
			loading,
			error,
			hasMore,
			handleNotificationClick,
			handleMarkAllAsRead,
			handleDelete,
			handleLoadMore,
			handleRetry,
			handleGoToSettings,
		};
	},
});
</script>

<style lang="scss" scoped>
.notifications-page {
	max-width: 800px;
	margin: 0 auto;
	padding: 2rem;
}

.page-header {
	display: flex;
	align-items: center;
	justify-content: space-between;
	margin-bottom: 2rem;
}

.page-title {
	font-size: 2rem;
	font-weight: 700;
	margin: 0;
}

.header-actions {
	display: flex;
	gap: 0.75rem;
	align-items: center;
}

.settings-btn {
	display: flex;
	align-items: center;
	gap: 0.5rem;
	padding: 0.5rem 1rem;
	background-color: white;
	color: var(--tmgr-blue);
	border: 2px solid var(--tmgr-blue);
	border-radius: 0.375rem;
	cursor: pointer;
	font-weight: 500;
	transition: background-color 0.2s;

	&:hover {
		background-color: rgba(59, 130, 246, 0.1);
	}

	.dark & {
		background-color: transparent;
		border-color: var(--tmgr-light-blue);
		color: var(--tmgr-light-blue);

		&:hover {
			background-color: rgba(96, 165, 250, 0.1);
		}
	}
}

.mark-all-btn {
	padding: 0.5rem 1rem;
	background-color: var(--tmgr-blue);
	color: white;
	border: none;
	border-radius: 0.375rem;
	cursor: pointer;
	font-weight: 500;
	transition: opacity 0.2s;

	&:hover:not(:disabled) {
		opacity: 0.9;
	}

	&:disabled {
		opacity: 0.6;
		cursor: not-allowed;
	}
}

.loading-state,
.error-state,
.empty-state {
	display: flex;
	flex-direction: column;
	align-items: center;
	justify-content: center;
	padding: 4rem 2rem;
	gap: 1rem;
	text-align: center;
}

.spinner {
	width: 48px;
	height: 48px;
	border: 4px solid rgba(0, 0, 0, 0.1);
	border-top-color: var(--tmgr-blue);
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
	margin-bottom: 1rem;
}

.empty-state {
	h2 {
		font-size: 1.5rem;
		font-weight: 600;
		margin: 0 0 0.5rem;
	}

	p {
		color: rgba(0, 0, 0, 0.6);
		margin: 0;
	}

	.dark & p {
		color: rgba(255, 255, 255, 0.6);
	}
}

.retry-btn {
	padding: 0.5rem 1rem;
	background-color: var(--tmgr-blue);
	color: white;
	border: none;
	border-radius: 0.375rem;
	cursor: pointer;
	font-weight: 500;
	transition: opacity 0.2s;

	&:hover {
		opacity: 0.9;
	}
}

.notifications-list {
	background: white;
	border-radius: 0.5rem;
	overflow: hidden;
	box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);

	.dark & {
		background: #1f2937;
	}
}

.load-more-section {
	display: flex;
	justify-content: center;
	padding: 1.5rem;
	border-top: 1px solid rgba(0, 0, 0, 0.1);

	.dark & {
		border-top-color: rgba(255, 255, 255, 0.1);
	}
}

.load-more-btn {
	padding: 0.5rem 2rem;
	background-color: transparent;
	color: var(--tmgr-blue);
	border: 2px solid var(--tmgr-blue);
	border-radius: 0.375rem;
	cursor: pointer;
	font-weight: 500;
	transition: all 0.2s;

	&:hover:not(:disabled) {
		background-color: var(--tmgr-blue);
		color: white;
	}

	&:disabled {
		opacity: 0.6;
		cursor: not-allowed;
	}
}
</style>

