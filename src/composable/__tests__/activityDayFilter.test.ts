import { getActivityFeed, withRetry } from '@/actions/tmgr/dashboard';
import type { Activity, EventHandlers } from '@/types/dashboard';
import { useActivityFeed } from '../useActivityFeed';

jest.mock('@/actions/tmgr/dashboard', () => ({
	getActivityFeed: jest.fn(),
	withRetry: jest.fn(),
}));
jest.mock('@/store', () => ({ __esModule: true, default: { state: {} } }));
const subscribe = jest.fn((_wsId: number, _handlers: EventHandlers) => 'h');
jest.mock('../usePusher', () => ({
	usePusher: () => ({
		subscribeToWorkspace: subscribe,
		unsubscribeHandlerFromWorkspace: jest.fn(),
	}),
}));

const feedMock = getActivityFeed as jest.Mock;
const row = (id: number) => ({ id } as unknown as Activity);
const page = (ids: number[], current = 1, last = 1, total = ids.length) => ({
	success: true,
	data: {
		data: ids.map(row),
		meta: { current_page: current, last_page: last, total },
	},
});
const sentDate = (call: number) => feedMock.mock.calls[call][1].filters.date;

beforeEach(() => {
	feedMock.mockReset();
	subscribe.mockClear();
	(withRetry as jest.Mock).mockImplementation((op) => op());
});

test('date goes verbatim into the request and survives member changes and Load More', async () => {
	feedMock.mockResolvedValue(page([1], 1, 2, 3));
	const feed = useActivityFeed({ value: 7 });

	await feed.applyFilters({ date: '2026-10-12' });
	expect(sentDate(0)).toBe('2026-10-12');

	await feed.applyFilters({ user_id: 5 });
	expect(feedMock.mock.calls[1][1].filters).toMatchObject({
		date: '2026-10-12',
		user_id: 5,
	});

	feedMock.mockResolvedValue(page([2], 2, 2, 3));
	await feed.loadMore();
	expect(feedMock.mock.calls[2][1]).toMatchObject({
		page: 2,
		filters: { date: '2026-10-12' },
	});
	expect(feed.totalItems.value).toBe(3);

	await feed.applyFilters({ date: undefined });
	expect(sentDate(3)).toBeUndefined();
});

test('live events are not inserted while a day is selected', async () => {
	feedMock.mockResolvedValue(page([1], 1, 1, 1));
	const feed = useActivityFeed({ value: 7 });
	feed.enableRealTime(7);
	const handlers = subscribe.mock.calls[0][1];

	await feed.applyFilters({ date: '2026-10-12' });
	handlers.onActivityCreated?.(row(99));
	expect(feed.activities.value.map((a) => a.id)).toEqual([1]);
	expect(feed.totalItems.value).toBe(1);

	await feed.applyFilters({ date: undefined });
	handlers.onActivityCreated?.(row(99));
	expect(feed.activities.value.map((a) => a.id)).toEqual([99, 1]);
	expect(feed.totalItems.value).toBe(2);
});

test('a failed day load leaves no stale rows', async () => {
	feedMock.mockResolvedValue(page([1, 2]));
	const feed = useActivityFeed({ value: 7 });
	await feed.refresh();
	expect(feed.activities.value).toHaveLength(2);

	feedMock.mockResolvedValue({
		success: false,
		error: { message: 'The date field must be a valid date.' },
	});
	await feed.applyFilters({ date: '2026-13-01' });
	expect(feed.activities.value).toEqual([]);
	expect(feed.error.value?.message).toBe(
		'The date field must be a valid date.',
	);

	feedMock.mockResolvedValue(page([3]));
	await feed.applyFilters({ date: '2026-10-12' });
	feedMock.mockResolvedValue({ success: false, error: { message: 'down' } });
	await feed.refresh();
	expect(feed.activities.value).toEqual([]);
});

test('switching the day drops the previous rows before the new ones arrive', async () => {
	feedMock.mockResolvedValue(page([1, 2]));
	const feed = useActivityFeed({ value: 7 });
	await feed.refresh();

	let resolve: (v: unknown) => void = () => {};
	feedMock.mockReturnValue(new Promise((r) => (resolve = r)));
	const pending = feed.applyFilters({ date: '2026-10-12' });
	expect(feed.activities.value).toEqual([]);
	expect(feed.loading.value).toBe(true);
	resolve(page([5]));
	await pending;
	expect(feed.activities.value.map((a) => a.id)).toEqual([5]);
});
