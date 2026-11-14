import { getPagesTree } from '@/actions/tmgr/pages';
import { nextTick, ref } from 'vue';
import { usePagesTree } from '../usePagesTree';

jest.mock('@/actions/tmgr/pages', () => ({
	getPagesTree: jest.fn(),
	movePage: jest.fn(),
}));
jest.mock('../usePagesRealtime', () => ({ usePagesRealtime: jest.fn() }));

describe('usePagesTree when modules are not known', () => {
	beforeEach(() => {
		jest.useFakeTimers();
		(getPagesTree as jest.Mock).mockResolvedValue([]);
	});
	afterEach(() => {
		jest.useRealTimers();
		jest.clearAllMocks();
	});

	it('shows the error state after the wait instead of loading forever', async () => {
		const tree = usePagesTree(ref(-5), ref(false), () => {});
		expect(tree.loaded.value).toBe(false);
		await jest.advanceTimersByTimeAsync(60000);
		expect(tree.failed.value).toBe(true);
		expect(tree.loaded.value).toBe(true);
	});

	it('retry waits again, then loads once modules are known', async () => {
		const enabled = ref(false);
		const tree = usePagesTree(ref(-5), enabled, () => {});
		await jest.advanceTimersByTimeAsync(60000);
		await tree.refresh();
		expect(tree.loaded.value).toBe(false);
		enabled.value = true;
		await nextTick();
		await jest.advanceTimersByTimeAsync(0);
		expect(getPagesTree).toHaveBeenCalledTimes(1);
		expect(tree.failed.value).toBe(false);
		expect(tree.loaded.value).toBe(true);
	});
});
