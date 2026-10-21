jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { post: jest.fn() },
}));

import $axios from '@/plugins/axios';
import { createAskingHelpComment, createComment } from '../comments';

const post = $axios.post as jest.Mock;

beforeEach(() => {
	post.mockReset();
	post.mockResolvedValue({ data: { data: { id: 1 } } });
});

describe('createAskingHelpComment', () => {
	it('omits file_ids when none are given', async () => {
		await createAskingHelpComment(7, 'why?');
		expect(post).toHaveBeenCalledWith('/tasks/7/comments/help', {
			question: 'why?',
		});
	});

	it('omits file_ids when the list is empty', async () => {
		await createAskingHelpComment(7, 'why?', []);
		expect(post.mock.calls[0][1]).toEqual({ question: 'why?' });
	});

	it('sends file_ids when present', async () => {
		await createAskingHelpComment(7, 'why?', [3, 4]);
		expect(post).toHaveBeenCalledWith('/tasks/7/comments/help', {
			question: 'why?',
			file_ids: [3, 4],
		});
	});
});

describe('createComment', () => {
	it('passes file_ids through in the payload', async () => {
		await createComment(7, { message: 'hi', file_ids: [9] });
		expect(post).toHaveBeenCalledWith('/tasks/7/comments', {
			message: 'hi',
			file_ids: [9],
		});
	});
});
