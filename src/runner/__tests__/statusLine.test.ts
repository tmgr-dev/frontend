import { statusDetail, statusLine } from '../statusLine';

describe('statusLine', () => {
	it('describes each runner state', () => {
		expect(statusLine({ state: 'ready', version: '0.4.0', caps: [] })).toBe(
			'Ready v0.4.0',
		);
		expect(statusLine({ state: 'ready', caps: [] })).toBe('Ready');
		expect(statusLine({ state: 'incompatible', caps: [] })).toBe(
			'Incompatible version',
		);
		expect(statusLine({ state: 'absent', caps: [] })).toBe('Not running');
		expect(statusLine(null)).toBe('Checking…');
	});
});

describe('statusDetail', () => {
	it('returns the runner message only for incompatible', () => {
		expect(
			statusDetail({
				state: 'incompatible',
				caps: [],
				message: 'Update the TMGR runner',
			}),
		).toBe('Update the TMGR runner');
		expect(statusDetail({ state: 'incompatible', caps: [] })).toBe(
			'Update the TMGR runner',
		);
		expect(
			statusDetail({ state: 'absent', caps: [], message: 'x' }),
		).toBeNull();
		expect(statusDetail(null)).toBeNull();
	});

	it('sanitizes and caps the message', () => {
		const text = statusDetail({
			state: 'incompatible',
			caps: [],
			message: `a\u001b${'z'.repeat(400)}`,
		});
		expect(text?.length).toBe(161);
		expect(text).not.toContain('\u001b');
	});
});
