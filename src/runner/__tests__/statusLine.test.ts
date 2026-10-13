import { statusLine } from '../statusLine';

describe('statusLine', () => {
	it('describes each runner state', () => {
		expect(statusLine({ state: 'ready', version: '0.4.0', caps: [] })).toBe('Ready v0.4.0');
		expect(statusLine({ state: 'ready', caps: [] })).toBe('Ready');
		expect(statusLine({ state: 'incompatible', caps: [] })).toBe('Incompatible version');
		expect(statusLine({ state: 'absent', caps: [] })).toBe('Not running');
		expect(statusLine(null)).toBe('Checking…');
	});
});
