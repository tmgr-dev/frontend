<template>
	<div>
		<BaseLayout title="Notifications">
			<template #header-actions>
				<Button
					variant="outline"
					title="Notification Settings"
					@click="handleGoToSettings"
				>
					<Settings />
					<span>Settings</span>
				</Button>
				<Button
					v-if="unreadCount > 0"
					:disabled="loading"
					@click="handleMarkAllAsRead"
				>
					Mark all as read
				</Button>
			</template>
			<template #body>
				<div class="min-h-96">
					<AsyncContent
						:pending="loading"
						:loaded="loaded"
						:has-data="notifications.length > 0"
						:error="error"
						:retry="handleRetry"
						label="Loading notifications"
					>
						<div
							v-if="notifications.length === 0"
							class="flex flex-col items-center justify-center gap-4 px-8 py-16 text-center"
						>
							<Bell :size="64" class="mb-4 text-ink-subtle opacity-40" />
							<h2 class="mb-2 text-2xl font-semibold text-ink">
								No notifications
							</h2>
							<p class="text-ink-subtle">You're all caught up!</p>
						</div>

						<div
							v-else
							class="overflow-hidden rounded-lg border border-line bg-surface"
						>
							<NotificationItem
								v-for="notification in notifications"
								:key="notification.id"
								:notification="notification"
								@click="handleNotificationClick(notification)"
								@delete="handleDelete(notification.id)"
							/>

							<div v-if="hasMore" class="flex justify-center p-6">
								<Button
									variant="outline"
									class="px-8"
									:disabled="loading"
									@click="handleLoadMore"
								>
									{{ loading ? 'Loading...' : 'Load more' }}
								</Button>
							</div>
						</div>
					</AsyncContent>
				</div>
			</template>
		</BaseLayout>
	</div>
</template>

<script>
	import AsyncContent from '@/components/async/AsyncContent.vue';
	import BaseLayout from '@/components/layouts/BaseLayout.vue';
	import NotificationItem from '@/components/notifications/NotificationItem.vue';
	import { Button } from '@/components/ui/button';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { useNotifications } from '@/composable/useNotifications';
	import store from '@/store';
	import { notificationTarget } from '@/utils/notificationTypes';
	import { Bell, Settings } from 'lucide-vue-next';
	import { defineComponent, onMounted, ref } from 'vue';
	import { useRouter } from 'vue-router';

	export default defineComponent({
		name: 'NotificationsPage',
		components: {
			AsyncContent,
			BaseLayout,
			Bell,
			Button,
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
				currentPage,
				loadNotifications,
				loadMore,
				markNotificationAsRead,
				markAllNotificationsAsRead,
				removeNotification,
			} = useNotifications();

			const loaded = ref(false);
			const attemptedPage = ref(1);
			async function readPage(page = 1) {
				if (loading.value) return;
				attemptedPage.value = page;
				await loadNotifications(page);
				if (!error.value) loaded.value = true;
			}
			const handleNotificationClick = async (notification) => {
				try {
					if (!notification.read_at) {
						await markNotificationAsRead(notification.id);
					}

					const target = notificationTarget(
						notification,
						store.getters.currentWorkspace?.code ?? '',
					);
					if (target) router.push(target);
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
					await readPage(currentPage.value + 1);
				} catch (err) {
					console.error('Error loading more notifications:', err);
				}
			};

			const handleRetry = () => {
				void readPage(attemptedPage.value);
			};

			const handleGoToSettings = () => {
				router.push('/settings?tab=notification');
			};

			onMounted(() => {
				setDocumentTitle('Notifications');
				void readPage(1);
			});

			return {
				loaded,
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
