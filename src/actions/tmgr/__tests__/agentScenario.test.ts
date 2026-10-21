const post = jest.fn();
jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { post: (...a: any[]) => post(...a) },
}));

import { sendAgentMessage } from '../agent';

describe('sendAgentMessage', () => {
	beforeEach(() => {
		post.mockReset();
		post.mockResolvedValue({
			data: { data: { message_id: 1, pending_message_id: 2 } },
		});
	});

	it('sends plain content without scenario fields', async () => {
		await sendAgentMessage(5, 'hi');
		expect(post).toHaveBeenCalledWith('/agent/conversations/5/messages', {
			content: 'hi',
		});
	});

	it('sends the scenario id with the browser timezone', async () => {
		await sendAgentMessage(5, 'Morning summary', 'morning_summary');
		const body = post.mock.calls[0][1];
		expect(body.scenario).toBe('morning_summary');
		expect(body.content).toBe('Morning summary');
		expect(typeof body.timezone).toBe('string');
	});
});
