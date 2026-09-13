import { effectScope } from 'vue';
import { useNotifications } from '../useNotifications';
const subscribe = jest.fn(() => 'notification-owner');
const unsubscribe = jest.fn();
jest.mock('../usePusher', () => ({
	usePusher: () => ({
		subscribeToUser: subscribe,
		unsubscribeHandler: unsubscribe,
	}),
}));
jest.mock('@/actions/tmgr/notifications', () => ({}));
beforeEach(() => {
	jest.clearAllMocks();
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
	expect(jest.getTimerCount()).toBe(1);
	scope.stop();
	expect(unsubscribe).toHaveBeenLastCalledWith(
		'App.User.8',
		'notification-owner',
	);
	expect(jest.getTimerCount()).toBe(0);
});
