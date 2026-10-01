import {
	errorMessageFrom,
	googleLinkConfirmationMessage,
	isSafeReturnPath,
	retryAfterLabel,
	shouldShowVerifyBanner,
} from '../emailVerification';

const httpError = (status: number, data: Record<string, unknown>) => ({
	isAxiosError: true,
	response: { status, data },
});

describe('shouldShowVerifyBanner', () => {
	it('shows only for a logged-in user explicitly unverified and not dismissed', () => {
		expect(shouldShowVerifyBanner({ email_verified: false }, false)).toBe(true);
		expect(shouldShowVerifyBanner({ email_verified: false }, true)).toBe(false);
		expect(shouldShowVerifyBanner({ email_verified: true }, false)).toBe(false);
		expect(shouldShowVerifyBanner({}, false)).toBe(false);
		expect(shouldShowVerifyBanner(null, false)).toBe(false);
		expect(shouldShowVerifyBanner(undefined, false)).toBe(false);
	});
});

describe('googleLinkConfirmationMessage', () => {
	it('returns the server message for 409 email_link_confirmation_required', () => {
		expect(
			googleLinkConfirmationMessage(
				httpError(409, {
					error: 'email_link_confirmation_required',
					message: 'Check your inbox',
				}),
			),
		).toBe('Check your inbox');
	});

	it('uses a fallback text when the message is missing', () => {
		expect(
			googleLinkConfirmationMessage(
				httpError(409, { error: 'email_link_confirmation_required' }),
			),
		).toMatch(/confirm/i);
	});

	it('ignores other errors', () => {
		expect(
			googleLinkConfirmationMessage(httpError(409, { error: 'x' })),
		).toBeNull();
		expect(googleLinkConfirmationMessage(httpError(500, {}))).toBeNull();
		expect(googleLinkConfirmationMessage(new Error('x'))).toBeNull();
	});
});

describe('errorMessageFrom', () => {
	it('prefers the server message on 403 and falls back otherwise', () => {
		expect(
			errorMessageFrom(
				httpError(403, {
					message: 'Verify your email address before inviting people',
				}),
				'fallback',
			),
		).toBe('Verify your email address before inviting people');
		expect(errorMessageFrom(httpError(500, {}), 'fallback')).toBe('fallback');
		expect(errorMessageFrom(new Error('x'), 'fallback')).toBe('fallback');
	});
});

describe('retryAfterLabel', () => {
	it('formats seconds and minutes', () => {
		expect(retryAfterLabel(1)).toBe('1 second');
		expect(retryAfterLabel(42)).toBe('42 seconds');
		expect(retryAfterLabel(60)).toBe('1 minute');
		expect(retryAfterLabel(125)).toBe('3 minutes');
		expect(retryAfterLabel(0)).toBe('a moment');
	});
});

describe('isSafeReturnPath', () => {
	it('accepts only same-origin absolute paths', () => {
		expect(isSafeReturnPath('/email/verify?token=a')).toBe(true);
		expect(isSafeReturnPath('//evil.com')).toBe(false);
		expect(isSafeReturnPath('https://evil.com')).toBe(false);
		expect(isSafeReturnPath('/\\evil.com')).toBe(false);
		expect(isSafeReturnPath('')).toBe(false);
		expect(isSafeReturnPath(null)).toBe(false);
	});
});
