import { footerHeightVars } from '../bottomBar';

describe('footerHeightVars', () => {
	it('publishes the measured footer height for the composer to match', () => {
		expect(footerHeightVars(62)).toEqual({ '--task-footer-height': '62px' });
	});

	it('publishes nothing before the footer has been measured', () => {
		expect(footerHeightVars(0)).toEqual({});
	});

	it('ignores a nonsense measurement rather than pinning the bar to it', () => {
		expect(footerHeightVars(-10)).toEqual({});
		expect(footerHeightVars(Number.NaN)).toEqual({});
	});
});
