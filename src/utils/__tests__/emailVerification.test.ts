import {
	errorMessageFrom,
	isSafeReturnPath,
	linkConfirmationMessage,
	linkConfirmationNotice,
	linkConfirmationProvider,
	retryAfterLabel,
	saveReturnPath,
	shouldShowVerifyBanner,
	takeReturnPath,
	withoutTokenQuery,
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

describe('linkConfirmationMessage', () => {
	it('returns the signed-in guidance for 409 email_link_confirmation_required', () => {
		const message = linkConfirmationMessage(
			httpError(409, {
				error: 'email_link_confirmation_required',
				message: 'Check your inbox',
			}),
			'google',
		);
		expect(message).toContain('We emailed you a confirmation link.');
		expect(message).toContain('while signed in');
		expect(message).toContain('sign in with Google again');
		expect(message).toContain('Forgot password');
	});

	it('names the provider', () => {
		const error = httpError(409, { error: 'email_link_confirmation_required' });
		expect(linkConfirmationMessage(error, 'github')).toContain(
			'sign in with GitHub again',
		);
		expect(linkConfirmationMessage(error, 'apple')).toContain(
			'sign in with Apple again',
		);
	});

	it('ignores other errors', () => {
		expect(
			linkConfirmationMessage(httpError(409, { error: 'x' }), 'github'),
		).toBeNull();
		expect(linkConfirmationMessage(httpError(500, {}), 'github')).toBeNull();
		expect(linkConfirmationMessage(new Error('x'), 'github')).toBeNull();
	});
});

describe('linkConfirmationProvider', () => {
	it.each(['google', 'github', 'apple'] as const)('maps %s', (provider) => {
		expect(linkConfirmationProvider(`${provider}_link_confirmation`)).toBe(
			provider,
		);
	});

	it.each([
		'telegram_link_confirmation',
		'google',
		'github',
		'link_confirmation',
		'github_link_confirmation_x',
		'x_github_link_confirmation',
		'',
		null,
		undefined,
		['github_link_confirmation'],
	])('returns null for %p', (code) => {
		expect(linkConfirmationProvider(code as never)).toBeNull();
	});
});

describe('linkConfirmationNotice', () => {
	it('uses the provider label', () => {
		expect(linkConfirmationNotice('github')).toContain(
			'sign in with GitHub again',
		);
	});
});

describe('withoutTokenQuery', () => {
	it('drops only the token', () => {
		expect(withoutTokenQuery({ token: 'a', x: '1' })).toEqual({ x: '1' });
		expect(withoutTokenQuery({ token: 'a' })).toEqual({});
	});
});

describe('return path storage', () => {
	const store: Record<string, string> = {};
	beforeEach(() => {
		for (const k of Object.keys(store)) delete store[k];
		(global as any).sessionStorage = {
			getItem: (k: string) => (k in store ? store[k] : null),
			setItem: (k: string, v: string) => (store[k] = v),
			removeItem: (k: string) => delete store[k],
		};
	});
	afterEach(() => delete (global as any).sessionStorage);

	it('keeps the token in the saved path and clears it after use', () => {
		saveReturnPath('/auth/link-confirm?token=abc');
		expect(takeReturnPath()).toBe('/auth/link-confirm?token=abc');
		expect(takeReturnPath()).toBeNull();
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
