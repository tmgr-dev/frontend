<template>
	<div class="relative" data-tour="nav.notification-bell">
		<DropdownMenu @update:open="handleDropdownOpen">
			<DropdownMenuTrigger as-child>
				<button
					class="relative rounded-md p-2 transition-colors hover:bg-surface-hover"
					:class="{ 'has-new-notification': hasNewNotification }"
					aria-label="Notifications"
				>
					<Bell :size="20" :class="{ 'animate-bell': hasNewNotification }" />
					<span
						v-if="unreadCount > 0"
						class="absolute right-0.5 top-0.5 flex h-[18px] min-w-[18px] items-center justify-center rounded-pill px-1 text-[10px] font-semibold leading-none text-white"
						:class="
							hasNewNotification
								? 'animate-bounce-gentle bg-red-600 shadow-[0_0_8px] shadow-red-600/60'
								: 'bg-red-500'
						"
					>
						{{ unreadCount > 99 ? '99+' : unreadCount }}
					</span>
				</button>
			</DropdownMenuTrigger>

			<DropdownMenuContent
				class="notification-dropdown flex max-h-[600px] w-[380px] max-w-[calc(100vw-1rem)] flex-col overflow-hidden p-0 [&>[role=separator]]:mx-0"
				align="end"
				:side-offset="8"
			>
				<div class="flex items-center justify-between gap-2 px-4 py-3">
					<h3 class="m-0 text-base font-semibold text-ink">Notifications</h3>
					<Button
						v-if="unreadCount > 0"
						variant="ghost"
						size="sm"
						:disabled="loading"
						@click="handleMarkAllAsRead"
					>
						Mark all as read
					</Button>
				</div>

				<DropdownMenuSeparator class="bg-line" />

				<div
					v-if="loading && notifications.length === 0"
					class="flex flex-col items-center justify-center gap-4 p-8 text-ink-subtle"
				>
					<div
						class="h-8 w-8 animate-spin rounded-pill border-[3px] border-line border-t-ink-muted"
					></div>
					<p class="text-sm">Loading...</p>
				</div>

				<div
					v-else-if="notifications.length === 0"
					class="flex flex-col items-center justify-center gap-4 p-8 text-ink-subtle"
				>
					<Bell :size="48" class="opacity-40" />
					<p class="text-sm">No notifications</p>
				</div>

				<div v-else class="max-h-[480px] overflow-y-auto">
					<NotificationItem
						v-for="notification in displayedNotifications"
						:key="notification.id"
						:notification="notification"
						@click="handleNotificationClick(notification)"
						@delete="handleDelete(notification.id)"
					/>
				</div>

				<DropdownMenuSeparator
					v-if="notifications.length > 0"
					class="my-0 bg-line"
				/>

				<div v-if="notifications.length > 0" class="p-1.5">
					<Button variant="ghost" class="w-full" @click="handleViewAll">
						View all
					</Button>
				</div>
			</DropdownMenuContent>
		</DropdownMenu>
	</div>
</template>

<script>
	import { Button } from '@/components/ui/button';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuSeparator,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';
	import { useNotifications } from '@/composable/useNotifications';
	import { notificationTarget } from '@/utils/notificationTypes';
	import { Bell } from 'lucide-vue-next';
	import { computed, defineComponent, onMounted, onUnmounted } from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import { useStore } from 'vuex';
	import NotificationItem from './NotificationItem.vue';

	export default defineComponent({
		name: 'NotificationBell',
		components: {
			Bell,
			Button,
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
					(setting) => setting.key === 'current_workspace',
				)?.value;
				const currentWorkspace = workspaces.find(
					(ws) => Number(ws.id) === Number(currentWorkspaceId),
				);
				return currentWorkspace?.code || 'default';
			};

			const handleNotificationClick = async (notification) => {
				try {
					if (!notification.read_at) {
						await markNotificationAsRead(notification.id);
					}

					const target = notificationTarget(
						notification,
						String(getCurrentWorkspaceCode()),
					);
					if (target) router.push(target);
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
	.animate-bell {
		animation: bell-ring 0.5s ease-in-out;
		animation-iteration-count: 3;
	}

	.animate-bounce-gentle {
		animation: bounce-gentle 0.5s ease-in-out infinite;
	}

	@keyframes bell-ring {
		0%,
		100% {
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
		0%,
		100% {
			transform: translateY(0);
		}
		50% {
			transform: translateY(-3px);
		}
	}
</style>
