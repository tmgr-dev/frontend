import $axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';
import { getPagesTree } from '../pages';

jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn() },
}));

jest.mock('@/store', () => ({
	__esModule: true,
	default: { commit: jest.fn() },
}));

jest.mock('../files', () => ({}));

describe('getPagesTree', () => {
	beforeEach(() => {
		jest.useFakeTimers();
		requestCache.clear();
	});
	afterEach(() => {
		jest.useRealTimers();
		jest.clearAllMocks();
	});

	it('rejects a request that never settles and lets the retry start a fresh one', async () => {
		($axios.get as jest.Mock).mockReturnValueOnce(new Promise(() => {}));
		const first = getPagesTree(false);
		const settled = expect(first).rejects.toThrow();
		await jest.advanceTimersByTimeAsync(60000);
		await settled;

		($axios.get as jest.Mock).mockResolvedValueOnce({ data: { data: [] } });
		await expect(getPagesTree(false)).resolves.toEqual([]);
		expect($axios.get).toHaveBeenCalledTimes(2);
	});
});
