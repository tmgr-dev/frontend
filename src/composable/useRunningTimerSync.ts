import store from '@/store';
import { onScopeDispose, watch } from 'vue';
import { usePusher } from './usePusher';

const WAKE_THROTTLE_MS = 1000;

interface RunningTimerSyncOptions {
	onEvent?: (task: any) => void;
	onResync: () => void;
}

export function useRunningTimerSync({
	onEvent,
	onResync,
}: RunningTimerSyncOptions): void {
	const { subscribeToUser, unsubscribeHandler, connectionState, reconnect } =
		usePusher();
	let channelName: string | null = null;
	let subscriptionId = '';
	let lastResync = -Infinity;

	const unsubscribe = (): void => {
		if (channelName && subscriptionId) {
			unsubscribeHandler(channelName, subscriptionId);
		}
		channelName = null;
		subscriptionId = '';
	};

	const stopWatching = watch(
		() => store.state.user?.id,
		(userId) => {
			unsubscribe();
			if (!userId) return;
			channelName = `App.User.${userId}`;
			subscriptionId = subscribeToUser(userId, {
				onTaskCountdownStarted: (task) => onEvent?.(task),
				onTaskCountdownStopped: (task) => onEvent?.(task),
				onReconnect: onResync,
			});
		},
		{ immediate: true },
	);

	const wake = (): void => {
		const now = Date.now();
		if (now - lastResync < WAKE_THROTTLE_MS) return;
		lastResync = now;
		if (connectionState.value !== 'connected') reconnect();
		onResync();
	};

	const onVisibilityChange = (): void => {
		if (document.visibilityState === 'visible') wake();
	};

	window.addEventListener('focus', wake);
	window.addEventListener('online', wake);
	document.addEventListener('visibilitychange', onVisibilityChange);

	onScopeDispose(() => {
		stopWatching();
		unsubscribe();
		window.removeEventListener('focus', wake);
		window.removeEventListener('online', wake);
		document.removeEventListener('visibilitychange', onVisibilityChange);
	});
}
