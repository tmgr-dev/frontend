jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn(), post: jest.fn(), put: jest.fn(), delete: jest.fn() },
}));
jest.mock('@/store', () => ({
	__esModule: true,
	default: { commit: jest.fn(), state: {} },
}));

jest.mock('@/utils/objectToQueryString', () => ({
	__esModule: true,
	default: jest.fn(),
}));

import axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';
import { assignPersonaToTask, unassignPersonaFromTask } from '../tasks';

const TASK = { id: 9, assignees: [], persona_assignees: [] };

beforeEach(() => {
	requestCache.clear();
	jest.clearAllMocks();
});

describe('assignPersonaToTask / unassignPersonaFromTask', () => {
	it('POSTs the persona onto the task and returns the updated task', async () => {
		(axios.post as jest.Mock).mockResolvedValue({ data: { data: TASK } });
		const result = await assignPersonaToTask(9, 'uuid-1');
		expect(axios.post).toHaveBeenCalledWith('tasks/9/personas/uuid-1');
		expect(result).toEqual(TASK);
	});

	it('DELETEs the persona from the task and returns the updated task', async () => {
		(axios.delete as jest.Mock).mockResolvedValue({ data: { data: TASK } });
		const result = await unassignPersonaFromTask(9, 'uuid-1');
		expect(axios.delete).toHaveBeenCalledWith('tasks/9/personas/uuid-1');
		expect(result).toEqual(TASK);
	});

	it('invalidates the cached task and status lists', async () => {
		requestCache.set('task-9', TASK);
		requestCache.set('tasks-status-3', []);
		(axios.post as jest.Mock).mockResolvedValue({ data: { data: TASK } });
		await assignPersonaToTask(9, 'uuid-1');
		expect(requestCache.has('task-9')).toBe(false);
		expect(requestCache.has('tasks-status-3')).toBe(false);
	});
});
