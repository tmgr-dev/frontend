import { getNotifications, getUnreadCount } from '@/actions/tmgr/notifications';
import { readFileSync } from 'fs';
import { join } from 'path';
import { effectScope, nextTick, ref } from 'vue';
import { useNotifications } from '../useNotifications';
const subscribe = jest.fn(() => 'notification-owner');
const unsubscribe = jest.fn();
jest.mock('../usePusher', () => ({
	usePusher: () => ({
		subscribeToUser: subscribe,
		unsubscribeHandler: unsubscribe,
	}),
}));
jest.mock('@/actions/tmgr/notifications', () => ({
	getNotifications: jest.fn(),
	getUnreadCount: jest.fn(),
	markAsRead: jest.fn(),
	markAllAsRead: jest.fn(),
	deleteNotification: jest.fn(),
}));
const invalidate = jest.fn();
jest.mock('@/utils/requestCache', () => ({
	requestCache: { invalidate: (key: string) => invalidate(key) },
}));
beforeEach(() => {
	jest.clearAllMocks();
	(getNotifications as jest.Mock).mockResolvedValue({
		data: [],
		current_page: 1,
		last_page: 1,
	});
	(getUnreadCount as jest.Mock).mockResolvedValue(0);
	jest.useFakeTimers();
});
afterEach(() => jest.useRealTimers());

test('notification cleanup removes its own handler in the original user channel', () => {
	const scope = effectScope();
	const notifications = scope.run(() => useNotifications())!;
	notifications.subscribeToRealtime(7);
	notifications.subscribeToRealtime(7);
	expect(subscribe).toHaveBeenCalledTimes(1);
	notifications.unsubscribeFromRealtime(8);
	expect(unsubscribe).toHaveBeenCalledWith('App.User.7', 'notification-owner');
	scope.stop();
});

test('user switch replaces subscription, and scope disposal releases handler and pulse timer', () => {
	const scope = effectScope();
	const notifications = scope.run(() => useNotifications())!;
	notifications.subscribeToRealtime(7);
	notifications.subscribeToRealtime(8);
	expect(unsubscribe).toHaveBeenCalledWith('App.User.7', 'notification-owner');
	expect(subscribe).toHaveBeenCalledTimes(2);
	const handlers = (subscribe.mock.calls[1] as unknown as [number, any])[1];
	handlers.onNotificationCreated({ notification: { id: 1, read_at: null } });
	expect(invalidate).toHaveBeenCalledWith('notifications-unread-count');
	expect(jest.getTimerCount()).toBe(1);
	scope.stop();
	expect(unsubscribe).toHaveBeenLastCalledWith(
		'App.User.8',
		'notification-owner',
	);
	expect(jest.getTimerCount()).toBe(0);
});

const flush = async () => {
	for (let i = 0; i < 10; i++) await Promise.resolve();
};

const page = (ids: number[], last = 1) => ({
	data: ids.map((id) => ({ id, read_at: null })),
	current_page: 1,
	last_page: last,
});

test('followUser subscribes, switches and resets state on account change', async () => {
	(getNotifications as jest.Mock).mockResolvedValue(page([1, 2], 3));
	(getUnreadCount as jest.Mock).mockResolvedValue(5);
	const id = ref<number | null>(7);
	const scope = effectScope();
	const n = scope.run(() => {
		const r = useNotifications();
		r.followUser(() => id.value);
		return r;
	})!;
	expect(subscribe).toHaveBeenLastCalledWith(7, expect.any(Object));
	await flush();
	expect(n.notifications.value).toHaveLength(2);
	expect(n.unreadCount.value).toBe(5);

	let resolveList!: (v: unknown) => void;
	let resolveCount!: (v: unknown) => void;
	(getNotifications as jest.Mock).mockImplementationOnce(
		() => new Promise((r) => (resolveList = r)),
	);
	(getUnreadCount as jest.Mock).mockImplementationOnce(
		() => new Promise((r) => (resolveCount = r)),
	);
	id.value = 8;
	await nextTick();
	expect(unsubscribe).toHaveBeenCalledWith('App.User.7', 'notification-owner');
	expect(subscribe).toHaveBeenLastCalledWith(8, expect.any(Object));
	expect(n.notifications.value).toEqual([]);
	expect(n.unreadCount.value).toBe(0);
	expect(n.totalPages.value).toBe(1);
	resolveList(page([9]));
	resolveCount(1);
	await flush();
	expect(n.notifications.value.map((x) => x.id)).toEqual([9]);
	expect(n.unreadCount.value).toBe(1);

	unsubscribe.mockClear();
	id.value = null;
	await nextTick();
	expect(unsubscribe).toHaveBeenCalledWith('App.User.8', 'notification-owner');
	expect(n.notifications.value).toEqual([]);
	expect(n.unreadCount.value).toBe(0);
	scope.stop();
});

test('responses for the previous account are discarded after a switch', async () => {
	let resolveList!: (v: unknown) => void;
	let resolveCount!: (v: unknown) => void;
	(getNotifications as jest.Mock).mockImplementationOnce(
		() => new Promise((r) => (resolveList = r)),
	);
	(getUnreadCount as jest.Mock).mockImplementationOnce(
		() => new Promise((r) => (resolveCount = r)),
	);
	const id = ref<number | null>(7);
	const scope = effectScope();
	const n = scope.run(() => {
		const r = useNotifications();
		r.followUser(() => id.value);
		return r;
	})!;
	(getNotifications as jest.Mock).mockResolvedValue(page([20]));
	(getUnreadCount as jest.Mock).mockResolvedValue(2);
	id.value = 8;
	await nextTick();
	await flush();
	resolveList(page([10, 11], 4));
	resolveCount(99);
	await flush();
	expect(n.notifications.value.map((x) => x.id)).toEqual([20]);
	expect(n.totalPages.value).toBe(1);
	expect(n.unreadCount.value).toBe(2);
	scope.stop();
});

test('NotificationBell follows the store user instead of the dead user/getUser getter', () => {
	const src = readFileSync(
		join(__dirname, '../../components/notifications/NotificationBell.vue'),
		'utf8',
	);
	expect(src).not.toContain('user/getUser');
	expect(src).toContain('followUser(');
	expect(src).toContain('store.state.token');
});
