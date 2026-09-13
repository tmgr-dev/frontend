import * as api from '@/actions/tmgr/dashboard';
import { nextTick, ref } from 'vue';
import { useActivityFeed } from '../useActivityFeed';
import { useDashboard } from '../useDashboard';
jest.mock('@/actions/tmgr/dashboard', () => ({
	getActivityFeed: jest.fn(),
	getDashboardStatistics: jest.fn(),
	getRecentTasks: jest.fn(),
	getTeamActivity: jest.fn(),
	getUserHeatmap: jest.fn(),
	withRetry: (fn: () => unknown) => fn(),
}));
jest.mock('@/store', () => ({
	__esModule: true,
	default: { state: {}, getters: {} },
}));
jest.mock('../usePusher', () => ({ usePusher: () => ({}) }));
const deferred = () => {
	let resolve!: (v: any) => void;
	let reject!: (v: any) => void;
	const promise = new Promise<any>((a, b) => {
		resolve = a;
		reject = b;
	});
	return { promise, resolve, reject };
};
beforeEach(() => jest.clearAllMocks());
test('dashboard ignores older heatmap responses and settles malformed results', async () => {
	const a = deferred(),
		b = deferred();
	(api.getUserHeatmap as jest.Mock)
		.mockReturnValueOnce(a.promise)
		.mockReturnValueOnce(b.promise);
	const dashboard = useDashboard(1);
	const first = dashboard.loadHeatmap({ year: 2025 });
	const second = dashboard.loadHeatmap({ year: 2026 });
	b.resolve({ success: true, data: { year: 2026 } });
	await second;
	a.resolve({ success: true, data: { year: 2025 } });
	await first;
	expect(dashboard.heatmapData.value).toEqual({ year: 2026 });
	(api.getUserHeatmap as jest.Mock).mockResolvedValue({ success: false });
	await dashboard.loadHeatmap();
	expect(dashboard.loadingStates.value.heatmap.isLoading).toBe(false);
});
test('dashboard cleanup invalidates pending responses', async () => {
	const a = deferred();
	(api.getUserHeatmap as jest.Mock).mockReturnValue(a.promise);
	const d = useDashboard(1);
	const pending = d.loadHeatmap();
	d.cleanup();
	a.resolve({ success: true, data: { year: 2025 } });
	await pending;
	expect(d.heatmapData.value).toBeNull();
});
test('feed filters fetch once and supersede old pagination', async () => {
	const a = deferred(),
		b = deferred();
	(api.getActivityFeed as jest.Mock)
		.mockResolvedValueOnce({
			success: true,
			data: {
				data: [{ id: 1 }],
				meta: { current_page: 1, last_page: 3, total: 3 },
			},
		})
		.mockReturnValueOnce(a.promise)
		.mockReturnValueOnce(b.promise);
	const f = useActivityFeed(1);
	await f.loadActivities(true);
	const old = f.loadMore();
	const fresh = f.applyFilters({ type: 'task_created' } as any);
	b.resolve({
		success: true,
		data: {
			data: [{ id: 3 }],
			meta: { current_page: 1, last_page: 1, total: 1 },
		},
	});
	await fresh;
	a.resolve({
		success: true,
		data: {
			data: [{ id: 2 }],
			meta: { current_page: 2, last_page: 3, total: 3 },
		},
	});
	await old;
	await nextTick();
	expect(f.activities.value).toEqual([{ id: 3 }]);
	expect(f.loadingMore.value).toBe(false);
	expect(api.getActivityFeed).toHaveBeenCalledTimes(3);
});
test('workspace change clears feed and ignores old responses', async () => {
	const a = deferred();
	(api.getActivityFeed as jest.Mock).mockReturnValue(a.promise);
	const ws = ref(1),
		f = useActivityFeed(ws);
	const pending = f.loadActivities();
	ws.value = 2;
	await nextTick();
	a.resolve({
		success: true,
		data: {
			data: [{ id: 1 }],
			meta: { current_page: 1, last_page: 1, total: 1 },
		},
	});
	await pending;
	expect(f.activities.value).toEqual([]);
	expect(f.loading.value).toBe(false);
});
