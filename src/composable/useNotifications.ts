import { ref, computed, onMounted, onUnmounted } from 'vue';
import {
	getNotifications,
	getUnreadCount,
	markAsRead,
	markAllAsRead,
	deleteNotification,
	type Notification,
	type NotificationPaginatedResponse,
} from '@/actions/tmgr/notifications';
import { usePusher } from '@/composable/usePusher';
import type { ActionError } from '@/types/api';

interface UseNotificationsReturn {
	notifications: import('vue').Ref<Notification[]>;
	unreadCount: import('vue').Ref<number>;
	loading: import('vue').Ref<boolean>;
	error: import('vue').Ref<ActionError | null>;
	currentPage: import('vue').Ref<number>;
	totalPages: import('vue').Ref<number>;
	hasMore: import('vue').ComputedRef<boolean>;
	hasNewNotification: import('vue').Ref<boolean>;
	loadNotifications: (page?: number) => Promise<void>;
	loadMore: () => Promise<void>;
	refreshUnreadCount: () => Promise<void>;
	markNotificationAsRead: (id: number) => Promise<void>;
	markAllNotificationsAsRead: () => Promise<void>;
	removeNotification: (id: number) => Promise<void>;
	subscribeToRealtime: (userId: number) => void;
	unsubscribeFromRealtime: (userId: number) => void;
	clearNewNotificationIndicator: () => void;
}

export function useNotifications(): UseNotificationsReturn {
	const notifications = ref<Notification[]>([]);
	const unreadCount = ref<number>(0);
	const loading = ref<boolean>(false);
	const error = ref<ActionError | null>(null);
	const currentPage = ref<number>(1);
	const totalPages = ref<number>(1);
	const hasNewNotification = ref<boolean>(false);
	const perPage = 20;

	const pusher = usePusher();
	let isSubscribed = false;
	let pulseTimeout: ReturnType<typeof setTimeout> | null = null;

	const hasMore = computed(() => currentPage.value < totalPages.value);

	const loadNotifications = async (page: number = 1): Promise<void> => {
		try {
			loading.value = true;
			error.value = null;

			const response: NotificationPaginatedResponse = await getNotifications(page, perPage);

			if (page === 1) {
				notifications.value = response.data;
			} else {
				notifications.value = [...notifications.value, ...response.data];
			}

			currentPage.value = response.current_page;
			totalPages.value = response.last_page;
		} catch (err: any) {
			error.value = {
				message: err.response?.data?.message || 'Failed to load notifications',
				type: 'api',
				timestamp: new Date().toISOString(),
				recoverable: true,
			};
			console.error('Error loading notifications:', err);
		} finally {
			loading.value = false;
		}
	};

	const loadMore = async (): Promise<void> => {
		if (hasMore.value && !loading.value) {
			await loadNotifications(currentPage.value + 1);
		}
	};

	const refreshUnreadCount = async (): Promise<void> => {
		try {
			unreadCount.value = await getUnreadCount();
		} catch (err) {
			console.error('Error refreshing unread count:', err);
		}
	};

	const markNotificationAsRead = async (id: number): Promise<void> => {
		try {
			await markAsRead(id);

			const notification = notifications.value.find((n) => n.id === id);
			if (notification && !notification.read_at) {
				notification.read_at = new Date().toISOString();
				unreadCount.value = Math.max(0, unreadCount.value - 1);
			}
		} catch (err) {
			console.error('Error marking notification as read:', err);
			throw err;
		}
	};

	const markAllNotificationsAsRead = async (): Promise<void> => {
		try {
			await markAllAsRead();

			notifications.value.forEach((notification) => {
				if (!notification.read_at) {
					notification.read_at = new Date().toISOString();
				}
			});

			unreadCount.value = 0;
		} catch (err) {
			console.error('Error marking all notifications as read:', err);
			throw err;
		}
	};

	const removeNotification = async (id: number): Promise<void> => {
		try {
			await deleteNotification(id);

			const index = notifications.value.findIndex((n) => n.id === id);
			if (index !== -1) {
				const notification = notifications.value[index];
				if (!notification.read_at) {
					unreadCount.value = Math.max(0, unreadCount.value - 1);
				}
				notifications.value.splice(index, 1);
			}
		} catch (err) {
			console.error('Error deleting notification:', err);
			throw err;
		}
	};

	const handleNewNotification = (notification: Notification) => {
		notifications.value.unshift(notification);

		if (!notification.read_at) {
			unreadCount.value += 1;
		}

		if (notifications.value.length > 100) {
			notifications.value = notifications.value.slice(0, 100);
		}
		
		hasNewNotification.value = true;
		if (pulseTimeout) {
			clearTimeout(pulseTimeout);
		}
		pulseTimeout = setTimeout(() => {
			hasNewNotification.value = false;
		}, 5000);
	};
	
	const clearNewNotificationIndicator = () => {
		hasNewNotification.value = false;
		if (pulseTimeout) {
			clearTimeout(pulseTimeout);
			pulseTimeout = null;
		}
	};

	const subscribeToRealtime = (userId: number): void => {
		if (isSubscribed) return;

		try {
			pusher.subscribeToUser(userId, {
				onNotificationCreated: (data: any) => {
					if (data.notification) {
						handleNewNotification(data.notification);
					}
				},
				onError: (err: ActionError) => {
					console.error('Real-time notification error:', err);
				},
			});

			isSubscribed = true;
			console.log('Subscribed to notification updates');
		} catch (err) {
			console.error('Failed to subscribe to notifications:', err);
		}
	};

	const unsubscribeFromRealtime = (userId: number): void => {
		if (!isSubscribed) return;

		try {
			pusher.unsubscribeFromUser(userId);
			isSubscribed = false;
			console.log('Unsubscribed from notification updates');
		} catch (err) {
			console.error('Failed to unsubscribe from notifications:', err);
		}
	};

	return {
		notifications,
		unreadCount,
		loading,
		error,
		currentPage,
		totalPages,
		hasMore,
		hasNewNotification,
		loadNotifications,
		loadMore,
		refreshUnreadCount,
		markNotificationAsRead,
		markAllNotificationsAsRead,
		removeNotification,
		subscribeToRealtime,
		unsubscribeFromRealtime,
		clearNewNotificationIndicator,
	};
}

