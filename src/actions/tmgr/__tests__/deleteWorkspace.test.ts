import $axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';
import { deleteWorkspace } from '../workspaces';

jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { delete: jest.fn() },
}));

jest.mock('@/store', () => ({ __esModule: true, default: {} }));

describe('deleteWorkspace', () => {
	afterEach(() => jest.clearAllMocks());

	it('sends the confirmation name and invalidates caches', async () => {
		($axios.delete as jest.Mock).mockResolvedValue({ data: { data: null } });
		const invalidate = jest.spyOn(requestCache, 'invalidate');

		await deleteWorkspace(5, 'My WS');

		expect($axios.delete).toHaveBeenCalledWith('workspaces/5', {
			data: { confirm_name: 'My WS' },
		});
		expect(invalidate).toHaveBeenCalledWith('workspaces');
		expect(invalidate).toHaveBeenCalledWith('workspace-5-members');
	});

	it('does not invalidate caches when the request fails', async () => {
		($axios.delete as jest.Mock).mockRejectedValue(new Error('422'));
		const invalidate = jest.spyOn(requestCache, 'invalidate');

		await expect(deleteWorkspace(5, 'x')).rejects.toThrow('422');
		expect(invalidate).not.toHaveBeenCalled();
	});
});
