jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { post: jest.fn(), get: jest.fn() },
}));

import $axios from '@/plugins/axios';
import { getCursorRepositories, launchCursorAgent } from '../cursor';

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

	it('sends repositories only when provided', async () => {
		await launchCursorAgent(9, 'main', 'implement', ['org/a']);

		expect($axios.post).toHaveBeenCalledWith('/tasks/9/cursor-agent', {
			source_branch: 'main',
			mode: 'implement',
			repositories: ['org/a'],
		});
	});
});

describe('getCursorRepositories', () => {
	it('unwraps data.repositories', async () => {
		const repositories = [
			{ full_name: 'org/a', default_branch: 'main', primary: true },
		];
		($axios.get as jest.Mock).mockResolvedValue({
			data: { data: { repositories } },
		});

		expect(await getCursorRepositories(9)).toEqual(repositories);
		expect($axios.get).toHaveBeenCalledWith(
			'/tasks/9/cursor-agent/repositories',
		);
	});
});
