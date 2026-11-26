import { invitationLoadError } from '../invitationLoadError';

const base = 'Failed to load invitation details';

describe('invitationLoadError', () => {
	it('shows the server message', () => {
		expect(
			invitationLoadError(
				{
					response: {
						status: 404,
						data: { message: 'Invalid invitation token' },
					},
				},
				base,
			),
		).toBe('Invalid invitation token');
	});

	it('adds the status when the response has no message', () => {
		expect(
			invitationLoadError({ response: { status: 502, data: '' } }, base),
		).toBe(`${base} (HTTP 502)`);
	});

	it('adds the error text when there is no response', () => {
		expect(
			invitationLoadError(
				new TypeError('Cannot read properties of undefined'),
				base,
			),
		).toBe(`${base}: Cannot read properties of undefined`);
	});

	it('falls back to the plain text', () => {
		expect(invitationLoadError({}, base)).toBe(base);
	});
});
