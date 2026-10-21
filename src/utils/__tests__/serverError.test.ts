import { serverMessageFrom } from '../serverError';

describe('serverMessageFrom', () => {
	it('returns the message from the response body', () => {
		const error = { response: { data: { message: 'Owner cannot leave.' } } };
		expect(serverMessageFrom(error, 'fallback')).toBe('Owner cannot leave.');
	});

	it('falls back when the body has no message', () => {
		expect(serverMessageFrom({ response: { data: {} } }, 'fallback')).toBe(
			'fallback',
		);
	});

	it('falls back when the message is blank or not a string', () => {
		expect(
			serverMessageFrom({ response: { data: { message: '  ' } } }, 'fallback'),
		).toBe('fallback');
		expect(
			serverMessageFrom({ response: { data: { message: 5 } } }, 'fallback'),
		).toBe('fallback');
	});

	it('falls back without a response or for non-objects', () => {
		expect(serverMessageFrom(new Error('x'), 'fallback')).toBe('fallback');
		expect(serverMessageFrom(null, 'fallback')).toBe('fallback');
		expect(serverMessageFrom(undefined, 'fallback')).toBe('fallback');
	});
});
