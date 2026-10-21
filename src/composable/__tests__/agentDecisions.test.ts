import { decideAgentActions, getAgentMessages } from '@/actions/tmgr/agent';

const mockPost = jest.fn();
const mockGet = jest.fn();

jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: {
		post: (...args: unknown[]) => mockPost(...args),
		get: (...args: unknown[]) => mockGet(...args),
	},
}));

describe('agent decision actions', () => {
	beforeEach(() => {
		mockPost.mockReset();
		mockGet.mockReset();
	});

	it('posts all action ids to the confirm endpoint and normalizes the reply', async () => {
		mockPost.mockResolvedValue({
			data: {
				data: { actions: [{ id: 1, risk: 'HIGH', status: 'executed' }] },
			},
		});
		const actions = await decideAgentActions(7, 'confirm', [1, 2]);
		expect(mockPost).toHaveBeenCalledWith('/agent/messages/7/actions/confirm', {
			action_ids: [1, 2],
		});
		expect(actions[0].risk).toBe('high');
	});

	it('posts to the cancel endpoint', async () => {
		mockPost.mockResolvedValue({ data: { data: { actions: [] } } });
		await expect(decideAgentActions(7, 'cancel', [3])).resolves.toEqual([]);
		expect(mockPost).toHaveBeenCalledWith('/agent/messages/7/actions/cancel', {
			action_ids: [3],
		});
	});

	it('normalizes pending actions of loaded messages', async () => {
		mockGet.mockResolvedValue({
			data: {
				data: [
					{ id: 1, status: 'done', steps: [] },
					{
						id: 2,
						status: 'awaiting',
						steps: [],
						pending_actions: [{ id: 9, tool_name: 'x' }],
					},
				],
			},
		});
		const messages = await getAgentMessages(5);
		expect(messages[0].pending_actions).toBeUndefined();
		expect(messages[1].pending_actions?.[0].tool).toBe('x');
	});
});
