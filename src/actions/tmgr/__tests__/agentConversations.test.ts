const get = jest.fn();
jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: (...a: any[]) => get(...a) },
}));

import { listConversations } from '../agent';

describe('listConversations', () => {
	beforeEach(() => {
		get.mockReset();
	});

	it('returns the items and the next cursor', async () => {
		get.mockResolvedValue({
			data: { data: [{ id: 3 }], next_cursor: '1790000000_3' },
		});
		const result = await listConversations(7);
		expect(result).toEqual({
			items: [{ id: 3 }],
			nextCursor: '1790000000_3',
		});
		expect(get.mock.calls[0][1].params).toEqual({
			workspaceId: 7,
			archived: false,
			limit: 50,
		});
	});

	it('sends the cursor as before and maps a missing cursor to null', async () => {
		get.mockResolvedValue({ data: { data: [] } });
		const result = await listConversations(7, {
			q: 'plan',
			archived: true,
			before: '1790000000_3',
		});
		expect(result.nextCursor).toBeNull();
		expect(get.mock.calls[0][1].params).toMatchObject({
			q: 'plan',
			archived: true,
			before: '1790000000_3',
		});
	});
});
