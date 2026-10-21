import { canLeaveWorkspace } from '../workspaceLeave';

const member = { user_id: 7 };

describe('canLeaveWorkspace', () => {
	it('shows Leave for a member', () => {
		expect(canLeaveWorkspace(member, 1)).toBe(true);
	});

	it('hides Leave for the owner', () => {
		expect(canLeaveWorkspace({ user_id: 1 }, 1)).toBe(false);
	});

	it('compares ids across string and number', () => {
		expect(canLeaveWorkspace({ user_id: '1' }, 1)).toBe(false);
		expect(canLeaveWorkspace({ user_id: 1 }, '1')).toBe(false);
		expect(canLeaveWorkspace({ user_id: '7' }, 1)).toBe(true);
	});

	it('hides Leave for default workspaces', () => {
		expect(canLeaveWorkspace({ ...member, type: 'default' }, 1)).toBe(false);
		expect(canLeaveWorkspace({ ...member, is_default: true }, 1)).toBe(false);
	});

	it('hides Leave for local workspaces', () => {
		expect(canLeaveWorkspace({ ...member, is_local: true }, 1)).toBe(false);
	});

	it('hides Leave when the owner is unknown', () => {
		expect(canLeaveWorkspace({}, 1)).toBe(false);
		expect(canLeaveWorkspace({ user_id: null }, 1)).toBe(false);
	});

	it('hides Leave when the current user is unknown', () => {
		expect(canLeaveWorkspace(member, undefined)).toBe(false);
	});
});
