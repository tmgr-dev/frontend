import $axios from '@/plugins/axios';
import store from '@/store';
import { requestCache } from '@/utils/requestCache';
import { emptyPagesTrash, permanentlyDeletePage } from '../pages';

jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { delete: jest.fn(), post: jest.fn() },
}));

jest.mock('@/store', () => ({
	__esModule: true,
	default: { commit: jest.fn() },
}));

jest.mock('../files', () => ({}));

describe('pages archive actions', () => {
	afterEach(() => jest.clearAllMocks());

	it('permanently deletes a page and refreshes pages caches', async () => {
		($axios.delete as jest.Mock).mockResolvedValue({
			data: { data: { deleted: 3, files: 1 } },
		});
		const invalidate = jest.spyOn(requestCache, 'invalidate');

		await expect(permanentlyDeletePage(12)).resolves.toEqual({
			deleted: 3,
			files: 1,
		});

		expect($axios.delete).toHaveBeenCalledWith('pages/12/permanent');
		expect(invalidate).toHaveBeenCalledWith(/^pages-/);
		expect(store.commit).toHaveBeenCalledWith('pagesEvent', {
			type: 'page.local',
			page: null,
		});
	});

	it('encodes a slug', async () => {
		($axios.delete as jest.Mock).mockResolvedValue({
			data: { data: { deleted: 1, files: 0 } },
		});

		await permanentlyDeletePage('a b');

		expect($axios.delete).toHaveBeenCalledWith('pages/a%20b/permanent');
	});

	it('empties the archive', async () => {
		($axios.post as jest.Mock).mockResolvedValue({
			data: { data: { purged: 2, deleted: 5, skipped: 1 } },
		});

		await expect(emptyPagesTrash()).resolves.toEqual({
			purged: 2,
			deleted: 5,
			skipped: 1,
		});
		expect($axios.post).toHaveBeenCalledWith('pages/trash/empty');
		expect(store.commit).toHaveBeenCalledWith('pagesEvent', {
			type: 'page.local',
			page: null,
		});
	});

	it('rethrows a 403 and still drops pages caches, without a pages event', async () => {
		const error = { response: { status: 403, data: '' } };
		($axios.post as jest.Mock).mockRejectedValue(error);
		const invalidate = jest.spyOn(requestCache, 'invalidate');

		await expect(emptyPagesTrash()).rejects.toBe(error);
		expect(invalidate).toHaveBeenCalledWith(/^pages-/);
		expect(store.commit).not.toHaveBeenCalled();
	});
});
