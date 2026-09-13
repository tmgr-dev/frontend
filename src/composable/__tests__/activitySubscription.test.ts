jest.mock('@/actions/tmgr/dashboard', () => ({
	getActivityFeed: jest.fn(),
	withRetry: jest.fn(),
}));
jest.mock('@/store', () => ({ __esModule: true, default: { state: {} } }));
const subscribe = jest.fn(() => 'feed-handler');
const unsubscribeOwned = jest.fn();
const unsubscribeAll = jest.fn();
jest.mock('../usePusher', () => ({
	usePusher: () => ({
		subscribeToWorkspace: subscribe,
		unsubscribeHandlerFromWorkspace: unsubscribeOwned,
		unsubscribeFromWorkspace: unsubscribeAll,
	}),
}));
import { useActivityFeed } from '../useActivityFeed';

test('activity cleanup removes only its handler from the subscribed workspace', () => {
	const workspace = { value: 7 };
	const feed = useActivityFeed(workspace);
	feed.enableRealTime(7);
	workspace.value = 8;
	feed.disableRealTime();
	expect(unsubscribeOwned).toHaveBeenCalledWith(7, 'feed-handler');
	expect(unsubscribeAll).not.toHaveBeenCalled();
});
