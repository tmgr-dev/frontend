import { featureDescription } from '@/utils/featureToggleCopy';
import { useFeatureToggles } from '../useFeatureToggles';

const mockGetters: Record<string, any> = {};

jest.mock('vuex', () => ({
	useStore: () => ({ getters: mockGetters }),
}));

describe('graph workspace feature toggle', () => {
	beforeEach(() => {
		mockGetters['featureToggles/isWorkspaceFeatureEnabled'] = (key: string) =>
			key === 'board';
		mockGetters['featureToggles/isUserFeatureEnabled'] = () => true;
		mockGetters['featureToggles/isLoaded'] = true;
	});

	it('is off when the server has no row for it', () => {
		const { isFeatureEnabled } = useFeatureToggles();
		expect(isFeatureEnabled('graph')).toBe(false);
	});

	it('is on when the workspace toggle is on', () => {
		mockGetters['featureToggles/isWorkspaceFeatureEnabled'] = (key: string) =>
			key === 'graph';
		const { isFeatureEnabled } = useFeatureToggles();
		expect(isFeatureEnabled('graph')).toBe(true);
	});

	it('has settings copy', () => {
		expect(featureDescription('graph')).toContain('workspace Map');
	});
});
