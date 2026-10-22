import { getMemberStats, getMemberTasks } from '@/actions/tmgr/dashboard';
import { effectScope, reactive } from 'vue';
import { useMemberPage } from '../useMemberPage';

jest.mock('@/actions/tmgr/dashboard', () => ({
	getMemberStats: jest.fn(),
	getMemberTasks: jest.fn(),
}));
const mockReplace = jest.fn();
const mockRoute = reactive({ query: {} as Record<string, string> });
jest.mock('vue-router', () => ({
	useRoute: () => mockRoute,
	useRouter: () => ({ replace: mockReplace }),
}));

describe('member page reused across query changes (TM-543)', () => {
	beforeEach(() => {
		jest.resetAllMocks();
		mockRoute.query = { window: '30d', tab: 'touched' };
		(getMemberStats as jest.Mock).mockResolvedValue({
			success: true,
			data: { name: 'Member' },
		});
		(getMemberTasks as jest.Mock).mockResolvedValue({
			success: true,
			data: { data: [], total: 0 },
		});
	});

	it('falls back to 30d without an error when the backend rejects window=all', async () => {
		mockRoute.query = { window: 'all', tab: 'touched' };
		const validation = { success: false, error: { type: 'validation' } };
		(getMemberStats as jest.Mock).mockResolvedValue(validation);
		(getMemberTasks as jest.Mock).mockResolvedValue(validation);
		const scope = effectScope();
		const page = scope.run(() =>
			useMemberPage(
				() => 1,
				() => 2,
			),
		)!;
		await page.refresh();

		expect(mockReplace).toHaveBeenCalledWith({
			query: { window: '30d', tab: 'touched' },
		});
		expect(page.error.value).toBeNull();
		scope.stop();
	});

	it('reloads only tasks on tab change and stats + tasks on window change', async () => {
		const scope = effectScope();
		const page = scope.run(() =>
			useMemberPage(
				() => 1,
				() => 2,
			),
		)!;
		await page.refresh();
		jest.clearAllMocks();

		mockRoute.query = { window: '30d', tab: 'assigned' };
		expect(page.tasksLoaded.value).toBe(true);
		expect(getMemberStats).not.toHaveBeenCalled();
		expect(getMemberTasks).toHaveBeenCalledTimes(1);
		expect((getMemberTasks as jest.Mock).mock.calls[0][2]).toMatchObject({
			tab: 'assigned',
			window: '30d',
			page: 1,
		});

		jest.clearAllMocks();
		mockRoute.query = { window: '7d', tab: 'assigned' };
		expect(getMemberStats).toHaveBeenCalledTimes(1);
		expect((getMemberStats as jest.Mock).mock.calls[0][2]).toBe('7d');
		expect(getMemberTasks).toHaveBeenCalledTimes(1);
		expect((getMemberTasks as jest.Mock).mock.calls[0][2]).toMatchObject({
			tab: 'assigned',
			window: '7d',
		});
		scope.stop();
	});

	it('hides the tasks panel when the new tab fails to load', async () => {
		const scope = effectScope();
		const page = scope.run(() =>
			useMemberPage(
				() => 1,
				() => 2,
			),
		)!;
		await page.refresh();
		(getMemberTasks as jest.Mock).mockResolvedValue({
			success: false,
			error: { message: 'Tasks offline' },
		});
		mockRoute.query = { window: '30d', tab: 'assigned' };
		await Promise.resolve();
		await Promise.resolve();
		expect(page.tasksError.value).toBe('Tasks offline');
		expect(page.tasksLoaded.value).toBe(false);
		scope.stop();
	});

	it('falls back to the default tab for a bogus or missing tab without reloading stats', async () => {
		const scope = effectScope();
		const page = scope.run(() =>
			useMemberPage(
				() => 1,
				() => 2,
			),
		)!;
		await page.refresh();
		const defaultTab = page.tab.value;
		jest.clearAllMocks();

		mockRoute.query = { window: '30d', tab: 'bogus' };
		expect(page.tab.value).toBe(defaultTab);
		mockRoute.query = { window: '30d' };
		expect(page.tab.value).toBe(defaultTab);
		expect(getMemberStats).not.toHaveBeenCalled();
		scope.stop();
	});
});
