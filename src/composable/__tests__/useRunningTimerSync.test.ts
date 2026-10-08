import { effectScope, reactive, ref } from 'vue';
import { useRunningTimerSync } from '../useRunningTimerSync';

const state = reactive<{ user: { id: number } | null }>({ user: { id: 7 } });
const connectionState = ref('connected');
const usePusherMock = jest.fn();
const subscribe = jest.fn();
const unsubscribe = jest.fn();
const reconnect = jest.fn();
let subCounter = 0;

jest.mock('@/store', () => ({
	__esModule: true,
	default: {
		get state() {
			return state;
		},
	},
}));
jest.mock('../usePusher', () => ({
	usePusher: () => {
		usePusherMock();
		return {
			subscribeToUser: subscribe,
			unsubscribeHandler: unsubscribe,
			connectionState,
			reconnect,
		};
	},
}));

const win = new EventTarget();
const doc = Object.assign(new EventTarget(), { visibilityState: 'visible' });

beforeEach(() => {
	jest.clearAllMocks();
	jest.useFakeTimers();
	jest.setSystemTime(1_000_000);
	subCounter = 0;
	subscribe.mockImplementation(() => `sub-${++subCounter}`);
	state.user = { id: 7 };
	connectionState.value = 'connected';
	doc.visibilityState = 'visible';
	Object.assign(globalThis, { window: win, document: doc });
});
afterEach(() => jest.useRealTimers());

const mount = () => {
	const onEvent = jest.fn();
	const onResync = jest.fn();
	const scope = effectScope();
	scope.run(() => useRunningTimerSync({ onEvent, onResync }));
	const handlers = () => subscribe.mock.calls[0][1];
	return { onEvent, onResync, scope, handlers };
};

test('countdown started and stopped pass the task to onEvent', () => {
	const { onEvent, handlers, scope } = mount();
	handlers().onTaskCountdownStarted({ id: 1 });
	handlers().onTaskCountdownStopped({ id: 2 });
	expect(onEvent).toHaveBeenNthCalledWith(1, { id: 1 });
	expect(onEvent).toHaveBeenNthCalledWith(2, { id: 2 });
	scope.stop();
});

test('socket reconnect triggers a resync', () => {
	const { onResync, handlers, scope } = mount();
	handlers().onReconnect();
	expect(onResync).toHaveBeenCalledTimes(1);
	scope.stop();
});

test('focus and visibilitychange in one burst resync once, a later wake resyncs again', () => {
	const { onResync, scope } = mount();
	win.dispatchEvent(new Event('focus'));
	doc.dispatchEvent(new Event('visibilitychange'));
	expect(onResync).toHaveBeenCalledTimes(1);
	jest.setSystemTime(1_005_000);
	win.dispatchEvent(new Event('online'));
	expect(onResync).toHaveBeenCalledTimes(2);
	scope.stop();
});

test('a hidden visibilitychange does nothing', () => {
	const { onResync, scope } = mount();
	doc.visibilityState = 'hidden';
	doc.dispatchEvent(new Event('visibilitychange'));
	expect(onResync).not.toHaveBeenCalled();
	scope.stop();
});

test('wake reconnects a socket that is not connected, and leaves a live one alone', () => {
	const { scope } = mount();
	win.dispatchEvent(new Event('focus'));
	expect(reconnect).not.toHaveBeenCalled();
	connectionState.value = 'error';
	jest.setSystemTime(1_005_000);
	win.dispatchEvent(new Event('focus'));
	expect(reconnect).toHaveBeenCalledTimes(1);
	scope.stop();
});

test('user switch resubscribes and disposal unsubscribes and removes window listeners', () => {
	const { onResync, scope } = mount();
	state.user = { id: 8 };
	return Promise.resolve().then(() => {
		expect(unsubscribe).toHaveBeenCalledWith('App.User.7', 'sub-1');
		expect(subscribe).toHaveBeenLastCalledWith(8, expect.any(Object));
		scope.stop();
		expect(unsubscribe).toHaveBeenLastCalledWith('App.User.8', 'sub-2');
		jest.setSystemTime(1_005_000);
		win.dispatchEvent(new Event('focus'));
		expect(onResync).not.toHaveBeenCalled();
	});
});

test('without a logged-in user nothing touches the socket and wake events are ignored', () => {
	state.user = null;
	const { onResync, scope } = mount();
	win.dispatchEvent(new Event('focus'));
	expect(usePusherMock).not.toHaveBeenCalled();
	expect(subscribe).not.toHaveBeenCalled();
	expect(onResync).not.toHaveBeenCalled();
	expect(reconnect).not.toHaveBeenCalled();
	scope.stop();
});

test('wake does not abort a connect that is already in progress', () => {
	const { scope } = mount();
	connectionState.value = 'connecting';
	win.dispatchEvent(new Event('focus'));
	expect(reconnect).not.toHaveBeenCalled();
	scope.stop();
});
