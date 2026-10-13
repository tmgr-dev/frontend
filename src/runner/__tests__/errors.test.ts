import { describeRunnerError, sanitizeMessage } from '../errors';

const PROFILE = 'This agent profile is not available. Pick another profile.';

describe('describeRunnerError', () => {
	it.each(['cancelled', new Error('cancelled'), { message: 'cancelled' }])(
		'keeps cancellation silent (%p)',
		(error) => {
			expect(describeRunnerError(error)).toEqual({ cancelled: true, text: '' });
		},
	);

	it.each([
		['busy', 'The agent is busy, try again in a moment.'],
		['unknown_profile', PROFILE],
		['unknown_profile: nope', PROFILE],
		[
			'sandbox',
			'The sandbox blocked this run. Check the project folder and try again.',
		],
		['limit', 'Too many agents are running. Stop one and try again.'],
		[
			'invalid_params',
			'The runner rejected the request. Check the project folder and try again.',
		],
		['task not found', 'This task was not found in the local workspace.'],
	])('maps %s to a readable message', (code, text) => {
		expect(describeRunnerError(code)).toEqual({ cancelled: false, text });
		expect(describeRunnerError(new Error(code))).toEqual({
			cancelled: false,
			text,
		});
	});

	it('falls back to a generic message per action', () => {
		expect(describeRunnerError('disk on fire')).toEqual({
			cancelled: false,
			text: 'Could not start the agent: disk on fire',
		});
		expect(describeRunnerError('x', 'stop')).toEqual({
			cancelled: false,
			text: 'Could not stop the agent: x',
		});
		expect(describeRunnerError('x', 'input')).toEqual({
			cancelled: false,
			text: 'Could not send input to the agent: x',
		});
	});

	it('has a prefix for opening the terminal', () => {
		expect(describeRunnerError('nope', 'open')).toEqual({
			cancelled: false,
			text: 'Could not open the terminal: nope',
		});
	});

	it('does not treat a longer word as a known code', () => {
		expect(describeRunnerError('busybody')).toEqual({
			cancelled: false,
			text: 'Could not start the agent: busybody',
		});
	});

	it('handles empty errors', () => {
		expect(describeRunnerError('')).toEqual({
			cancelled: false,
			text: 'Could not start the agent.',
		});
	});
});

describe('sanitizeMessage', () => {
	it('strips control characters and collapses whitespace', () => {
		expect(sanitizeMessage('a\u001b[31m\n\n  b\u0000c')).toBe('a [31m b c');
	});

	it('caps the length', () => {
		const out = sanitizeMessage('x'.repeat(500));
		expect(out.length).toBe(161);
		expect(out.endsWith('…')).toBe(true);
	});
});
