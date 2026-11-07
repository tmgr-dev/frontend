const getTasks = jest.fn();

jest.mock('@/actions/tmgr/categories', () => ({ getCategories: jest.fn() }));
jest.mock('@/actions/tmgr/tasks', () => ({ getTasks }));
jest.mock('@/store', () => ({
	__esModule: true,
	default: { commit: jest.fn() },
}));
jest.mock('@/utils/taskWindow', () => ({
	isInDetachedWindow: jest.fn(),
	isInTaskWindow: jest.fn(),
	openTaskWindow: jest.fn(),
	taskWindowTarget: jest.fn(),
}));

import { openTaskByKey, resolveTaskByKey } from '../useTaskKeyLinks';

describe('resolveTaskByKey', () => {
	beforeEach(() => getTasks.mockReset());

	it('returns the task whose title carries the key', async () => {
		getTasks.mockResolvedValue({
			data: [
				{ id: 1, title: 'TM-15: other' },
				{ id: 2, title: 'TM-157: wanted' },
			],
		});
		expect((await resolveTaskByKey('TM-157'))?.id).toBe(2);
	});

	it('returns null when nothing matches', async () => {
		getTasks.mockResolvedValue({ data: [{ id: 1, title: 'TM-15: other' }] });
		expect(await resolveTaskByKey('TM-157')).toBeNull();
	});

	it('returns null when the request fails', async () => {
		getTasks.mockRejectedValue(new Error('offline'));
		expect(await resolveTaskByKey('TM-157')).toBeNull();
		expect(await openTaskByKey('TM-157')).toBe(false);
	});
});
