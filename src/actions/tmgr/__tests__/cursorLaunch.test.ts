jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { post: jest.fn() },
}));

import $axios from '@/plugins/axios';
import { launchCursorAgent } from '../cursor';

describe('launchCursorAgent', () => {
	beforeEach(() => {
		($axios.post as jest.Mock)
			.mockReset()
			.mockResolvedValue({ data: { agent: { id: 5 } } });
	});

	it('launches in implement mode by default', async () => {
		await launchCursorAgent(9, 'main');

		expect($axios.post).toHaveBeenCalledWith('/tasks/9/cursor-agent', {
			source_branch: 'main',
			mode: 'implement',
		});
	});

	it('sends research mode and returns the agent', async () => {
		const agent = await launchCursorAgent(9, 'main', 'research');

		expect($axios.post).toHaveBeenCalledWith('/tasks/9/cursor-agent', {
			source_branch: 'main',
			mode: 'research',
		});
		expect(agent).toEqual({ id: 5 });
	});
});
