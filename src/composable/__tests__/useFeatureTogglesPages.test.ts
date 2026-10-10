import { useFeatureToggles } from '../useFeatureToggles';

const mockGetters: Record<string, any> = {};

jest.mock('vuex', () => ({
	useStore: () => ({ getters: mockGetters }),
}));

describe('useFeatureToggles', () => {
	beforeEach(() => {
		mockGetters['featureToggles/isFeatureEnabled'] = (key: string) =>
			key !== 'pages';
		mockGetters['featureToggles/isLoaded'] = true;
	});

	it('asks the store for every key', () => {
		const { isFeatureEnabled } = useFeatureToggles();
		expect(isFeatureEnabled('pages')).toBe(false);
		expect(isFeatureEnabled('graph')).toBe(true);
	});

	it('answers workspace and user lookups the same way', () => {
		const toggles = useFeatureToggles();
		expect(toggles.isWorkspaceFeatureEnabled('pages')).toBe(false);
		expect(toggles.isUserFeatureEnabled('board.search_input')).toBe(true);
	});
});
