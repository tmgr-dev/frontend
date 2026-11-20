import $axios from '@/plugins/axios';
import { deleteCategory } from '../categories';

jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { delete: jest.fn() },
}));

describe('deleteCategory', () => {
	it('returns a deleted_at timestamp when the API answers 204 with an empty body', async () => {
		($axios.delete as jest.Mock).mockResolvedValue({ data: '', status: 204 });

		const deletedAt = await deleteCategory(7);

		expect($axios.delete).toHaveBeenCalledWith('project_categories/7');
		expect(Number.isNaN(Date.parse(deletedAt))).toBe(false);
	});
});
