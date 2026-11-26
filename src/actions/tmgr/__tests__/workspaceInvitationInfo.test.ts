import $axios from '@/plugins/axios';
import { workspaceInvitationInfo } from '../workspaces';

jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn() },
}));

jest.mock('@/store', () => ({ __esModule: true, default: {} }));

const info = {
	workspace: { id: 56, name: 'Team' },
	invitation: { token: 'id-89', email: 'b@example.com' },
};

describe('workspaceInvitationInfo', () => {
	it('unwraps the data envelope the API puts around every response', async () => {
		($axios.get as jest.Mock).mockResolvedValue({ data: { data: info } });

		const result = await workspaceInvitationInfo('id-89');

		expect($axios.get).toHaveBeenCalledWith(
			'workspaces/invitations/id-89/info',
		);
		expect(result.workspace.name).toBe('Team');
		expect(result.workspace.id).toBe(56);
	});

	it('keeps the already_member flag', async () => {
		($axios.get as jest.Mock).mockResolvedValue({
			data: { data: { ...info, already_member: true } },
		});

		expect((await workspaceInvitationInfo('id-89')).already_member).toBe(true);
	});
});
