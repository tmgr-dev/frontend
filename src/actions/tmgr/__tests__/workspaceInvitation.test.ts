jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn(), post: jest.fn() },
}));
jest.mock('@/store', () => ({
	__esModule: true,
	default: { getters: {}, state: {} },
}));

import $axios from '@/plugins/axios';
import {
	acceptWorkspaceInvitation,
	createWorkspaceInvitation,
	workspaceInvitationInfo,
} from '../workspaces';

const get = $axios.get as jest.Mock;
const post = $axios.post as jest.Mock;

beforeEach(() => {
	jest.clearAllMocks();
});

test.each(['tok123', 'id-89'])(
	'info requests the invitation %s',
	async (token) => {
		get.mockResolvedValue({
			data: {
				data: { workspace: { id: 1, name: 'Demo' }, already_member: false },
			},
		});

		const data = await workspaceInvitationInfo(token);

		expect(get).toHaveBeenCalledWith(`workspaces/invitations/${token}/info`);
		expect(data).toEqual({
			workspace: { id: 1, name: 'Demo' },
			already_member: false,
		});
	},
);

test.each(['tok123', 'id-89'])(
	'accept posts to the invitation %s',
	async (token) => {
		post.mockResolvedValue({ data: { data: { joined: true } } });

		const result = await acceptWorkspaceInvitation(token);

		expect(post).toHaveBeenCalledWith(`workspaces/invitations/${token}/accept`);
		expect(result).toEqual({ joined: true });
	},
);

test('create posts the payload to the workspace and returns the issued token', async () => {
	post.mockResolvedValue({
		data: { data: { token: 'id-89', usage_times: 0 } },
	});

	const response = await createWorkspaceInvitation(7, {
		max_usage_times: 1,
		expired_at: null,
	});

	expect(post).toHaveBeenCalledWith('/workspaces/7/invitations', {
		max_usage_times: 1,
		expired_at: null,
	});
	expect(response.token).toBe('id-89');
});
