import { featureDescription } from '@/utils/featureToggleCopy';

describe('graph settings copy', () => {
	it('has settings copy', () => {
		expect(featureDescription('graph')).toContain('workspace Map');
	});
});
