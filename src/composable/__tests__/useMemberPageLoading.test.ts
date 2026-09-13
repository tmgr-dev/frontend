import { getMemberStats, getMemberTasks } from '@/actions/tmgr/dashboard';
import { effectScope, reactive } from 'vue';
import { useMemberPage } from '../useMemberPage';

jest.mock('@/actions/tmgr/dashboard', () => ({
	getMemberStats: jest.fn(),
	getMemberTasks: jest.fn(),
}));
const mockRoute = reactive({ query: {} });
jest.mock('vue-router', () => ({
	useRoute: () => mockRoute,
	useRouter: () => ({ replace: jest.fn() }),
}));

describe('member loading boundaries', () => {
	beforeEach(() => jest.resetAllMocks());
	it('keeps independent task errors when member stats refresh successfully', async () => {
		(getMemberStats as jest.Mock).mockResolvedValue({
			success: true,
			data: { name: 'Member' },
		});
		(getMemberTasks as jest.Mock).mockResolvedValue({
			success: false,
			error: { message: 'Tasks offline' },
		});
		const scope = effectScope();
		const page = scope.run(() =>
			useMemberPage(
				() => 1,
				() => 2,
			),
		)!;
		await page.refresh();
		expect(page.stats.value?.name).toBe('Member');
		expect(page.tasksError.value).toBe('Tasks offline');
		expect(page.tasksLoaded.value).toBe(false);
		expect(page.tasksLoading.value).toBe(false);
		await page.loadStats();
		expect(page.tasksError.value).toBe('Tasks offline');
		(getMemberTasks as jest.Mock).mockResolvedValue({
			success: true,
			data: { data: [], total: 0 },
		});
		await page.retryTasks();
		expect(page.tasksLoaded.value).toBe(true);
		expect(page.tasksError.value).toBeNull();
		scope.stop();
	});
	it('retains existing tasks and retries the failed next page', async () => {
		(getMemberStats as jest.Mock).mockResolvedValue({
			success: true,
			data: { name: 'Member' },
		});
		(getMemberTasks as jest.Mock)
			.mockResolvedValueOnce({
				success: true,
				data: { data: [{ id: 1 }], total: 2 },
			})
			.mockResolvedValueOnce({ success: false, error: { message: 'Offline' } })
			.mockResolvedValueOnce({
				success: true,
				data: { data: [{ id: 2 }], total: 2 },
			});
		const scope = effectScope();
		const page = scope.run(() =>
			useMemberPage(
				() => 1,
				() => 2,
			),
		)!;
		await page.refresh();
		await page.loadMoreTasks();
		expect(page.tasks.value.map((task) => task.id)).toEqual([1]);
		expect(page.tasksLoaded.value).toBe(true);
		await page.retryTasks();
		expect(page.tasks.value.map((task) => task.id)).toEqual([1, 2]);
		expect(
			(getMemberTasks as jest.Mock).mock.calls.map((args) => args[2].page),
		).toEqual([1, 2, 2]);
		scope.stop();
	});
});
