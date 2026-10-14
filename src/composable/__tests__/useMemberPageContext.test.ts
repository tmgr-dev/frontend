import { getMemberStats, getMemberTasks } from '@/actions/tmgr/dashboard';
import { effectScope, reactive, ref } from 'vue';
import { useMemberPage } from '../useMemberPage';

jest.mock('@/actions/tmgr/dashboard', () => ({
	getMemberStats: jest.fn(),
	getMemberTasks: jest.fn(),
}));
const mockRoute = reactive({ query: {} as Record<string, string> });
jest.mock('vue-router', () => ({
	useRoute: () => mockRoute,
	useRouter: () => ({ replace: jest.fn() }),
}));

const deferred = () => {
	let resolve!: (value: unknown) => void;
	const promise = new Promise((r) => (resolve = r));
	return { promise, resolve };
};
const ok = (data: unknown) => ({ success: true, data });
const flush = async () => {
	for (let i = 0; i < 5; i++) await Promise.resolve();
};

describe('member page keeps context on period change (TM-551)', () => {
	const workspace = ref(1);
	const user = ref(2);
	let scope: ReturnType<typeof effectScope>;
	let page: ReturnType<typeof useMemberPage>;

	beforeEach(async () => {
		jest.resetAllMocks();
		workspace.value = 1;
		user.value = 2;
		mockRoute.query = { window: '30d', tab: 'touched' };
		(getMemberStats as jest.Mock).mockResolvedValue(ok({ name: '30d' }));
		(getMemberTasks as jest.Mock).mockResolvedValue(
			ok({ data: [{ id: 1 }], total: 1 }),
		);
		scope = effectScope();
		page = scope.run(() =>
			useMemberPage(
				() => workspace.value,
				() => user.value,
			),
		)!;
		await page.refresh();
		jest.clearAllMocks();
	});
	afterEach(() => scope.stop());

	it('keeps stats and the tasks panel while the new period loads', async () => {
		const stats = deferred();
		(getMemberStats as jest.Mock).mockReturnValueOnce(stats.promise);
		mockRoute.query = { window: '7d', tab: 'touched' };
		expect(page.stats.value?.name).toBe('30d');
		expect(page.statsLoading.value).toBe(true);
		expect(page.tasksLoaded.value).toBe(true);
		expect(page.tasks.value).toEqual([]);
		stats.resolve(ok({ name: '7d' }));
		await flush();
		expect(page.stats.value?.name).toBe('7d');
		expect(page.statsLoading.value).toBe(false);
		expect(page.tasksLoaded.value).toBe(true);
	});

	it('leaves stats untouched on tab change', () => {
		const before = page.stats.value;
		mockRoute.query = { window: '30d', tab: 'assigned' };
		expect(page.stats.value).toBe(before);
		expect(page.statsLoading.value).toBe(false);
		expect(getMemberStats).not.toHaveBeenCalled();
	});

	it('handles back/forward query mutations like clicks', async () => {
		const stats = deferred();
		(getMemberStats as jest.Mock).mockReturnValueOnce(stats.promise);
		mockRoute.query.window = '7d';
		expect(page.stats.value?.name).toBe('30d');
		expect(page.statsLoading.value).toBe(true);
		stats.resolve(ok({ name: '7d' }));
		await flush();
		expect(page.stats.value?.name).toBe('7d');
		jest.clearAllMocks();
		mockRoute.query.tab = 'assigned';
		expect(page.stats.value?.name).toBe('7d');
		expect(getMemberStats).not.toHaveBeenCalled();
		expect(getMemberTasks).toHaveBeenCalledTimes(1);
	});

	it.each([
		['member', () => (user.value = 3)],
		['workspace', () => (workspace.value = 2)],
	])(
		'clears stats on %s change and discards the previous member late response',
		async (_label, change) => {
			const late = deferred();
			const next = deferred();
			(getMemberStats as jest.Mock)
				.mockReturnValueOnce(late.promise)
				.mockReturnValueOnce(next.promise);
			mockRoute.query = { window: '7d', tab: 'touched' };
			change();
			expect(page.stats.value).toBeNull();
			expect(page.tasksLoaded.value).toBe(false);
			late.resolve(ok({ name: 'previous' }));
			await flush();
			expect(page.stats.value).toBeNull();
			next.resolve(ok({ name: 'next' }));
			await flush();
			expect(page.stats.value?.name).toBe('next');
		},
	);

	it('lets the last period win when responses arrive out of order', async () => {
		const first = deferred();
		const second = deferred();
		(getMemberStats as jest.Mock)
			.mockReturnValueOnce(first.promise)
			.mockReturnValueOnce(second.promise);
		mockRoute.query = { window: '7d', tab: 'touched' };
		mockRoute.query = { window: 'today', tab: 'touched' };
		second.resolve(ok({ name: 'today' }));
		await flush();
		first.resolve(ok({ name: '7d' }));
		await flush();
		expect(page.stats.value?.name).toBe('today');
		expect(page.statsLoading.value).toBe(false);
	});

	it('ignores a stale period response while the newer one is pending', async () => {
		const first = deferred();
		const second = deferred();
		(getMemberStats as jest.Mock)
			.mockReturnValueOnce(first.promise)
			.mockReturnValueOnce(second.promise);
		mockRoute.query = { window: '7d', tab: 'touched' };
		mockRoute.query = { window: 'today', tab: 'touched' };
		first.resolve(ok({ name: '7d' }));
		await flush();
		expect(page.stats.value?.name).toBe('30d');
		expect(page.statsLoading.value).toBe(true);
		second.resolve(ok({ name: 'today' }));
		await flush();
		expect(page.stats.value?.name).toBe('today');
		expect(page.statsLoading.value).toBe(false);
	});

	it('requests tasks for the new period', () => {
		mockRoute.query = { window: '7d', tab: 'touched' };
		expect(getMemberTasks).toHaveBeenCalledWith(
			1,
			2,
			expect.objectContaining({ window: '7d', tab: 'touched', page: 1 }),
		);
	});

	it('discards a late tasks response for the previous member', async () => {
		const late = deferred();
		const next = deferred();
		(getMemberTasks as jest.Mock)
			.mockReturnValueOnce(late.promise)
			.mockReturnValueOnce(next.promise);
		mockRoute.query = { window: '7d', tab: 'touched' };
		user.value = 3;
		late.resolve(ok({ data: [{ id: 'previous' }], total: 1 }));
		await flush();
		expect(page.tasks.value).toEqual([]);
		expect(page.tasksLoaded.value).toBe(false);
		next.resolve(ok({ data: [{ id: 'next' }], total: 1 }));
		await flush();
		expect(page.tasks.value).toEqual([{ id: 'next' }]);
		expect(page.tasksLoaded.value).toBe(true);
	});

	it('keeps previous stats when the new period request throws', async () => {
		(getMemberStats as jest.Mock).mockRejectedValueOnce(new Error('boom'));
		mockRoute.query = { window: '7d', tab: 'touched' };
		await flush();
		expect(page.stats.value?.name).toBe('30d');
		expect(page.statsError.value).toBe('boom');
		expect(page.statsLoading.value).toBe(false);
	});

	it('keeps previous stats and reports the error when the new period fails', async () => {
		(getMemberStats as jest.Mock).mockResolvedValueOnce({
			success: false,
			error: { message: 'Stats offline' },
		});
		mockRoute.query = { window: '7d', tab: 'touched' };
		await flush();
		expect(page.stats.value?.name).toBe('30d');
		expect(page.statsError.value).toBe('Stats offline');
		expect(page.statsLoading.value).toBe(false);
	});
});
