import { BYPASS_PHRASE, bypassAvailable, isBypassPhrase } from '../bypass';

describe('bypass consent', () => {
	it('accepts only the exact phrase', () => {
		expect(BYPASS_PHRASE).toBe('I AGREE');
		expect(isBypassPhrase('I AGREE')).toBe(true);
		for (const text of [
			'i agree',
			'I AGREE ',
			' I AGREE',
			'I  AGREE',
			'AGREE',
			'',
		])
			expect(isBypassPhrase(text)).toBe(false);
	});

	it('is available only when the runner advertises runs.bypass', () => {
		expect(bypassAvailable(null)).toBe(false);
		expect(bypassAvailable({ state: 'ready', caps: ['runs.create'] })).toBe(
			false,
		);
		expect(bypassAvailable({ state: 'ready', caps: ['runs.bypass'] })).toBe(
			true,
		);
		expect(bypassAvailable({ state: 'absent', caps: ['runs.bypass'] })).toBe(
			false,
		);
	});
});
