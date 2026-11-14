jest.mock('@/actions/tmgr/featureToggles', () => ({
	getUserFeatureToggles: jest.fn(),
	updateUserFeatureToggles: jest.fn(),
	updateWorkspaceFeatureToggles: jest.fn(),
}));
jest.mock('@/actions/tmgr/modules', () => ({
	getLegacyWorkspaceFeatureToggles: jest.fn(),
	getWorkspaceModules: jest.fn(),
}));
import featureToggles from '../featureToggles';

describe('enforceLandingPage', () => {
	it('keeps an account-wide landing page that this workspace cannot show', async () => {
		const dispatch = jest.fn();
		const allowed = await (featureToggles as any).actions.enforceLandingPage({
			state: { userToggles: { default_landing_page: { value: 'dashboard' } } },
			dispatch,
			getters: { isFeatureEnabled: () => false },
		});
		expect(dispatch).not.toHaveBeenCalled();
		expect(allowed).toEqual(['list']);
	});
});
