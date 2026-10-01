import { useFeatureToggles } from '../useFeatureToggles';

const mockGetters: Record<string, any> = {};

jest.mock('vuex', () => ({
	useStore: () => ({ getters: mockGetters }),
}));

describe('pages workspace feature toggle', () => {
	beforeEach(() => {
		mockGetters['featureToggles/isWorkspaceFeatureEnabled'] = (key: string) =>
			key === 'board';
		mockGetters['featureToggles/isUserFeatureEnabled'] = () => true;
		mockGetters['featureToggles/isLoaded'] = true;
	});

	it('is read from the workspace toggles instead of defaulting to enabled', () => {
		const { isFeatureEnabled } = useFeatureToggles();
		expect(isFeatureEnabled('pages')).toBe(false);
	});

	it('is enabled when the workspace toggle is on', () => {
		mockGetters['featureToggles/isWorkspaceFeatureEnabled'] = (key: string) =>
			key === 'pages';
		const { isFeatureEnabled } = useFeatureToggles();
		expect(isFeatureEnabled('pages')).toBe(true);
	});
});
