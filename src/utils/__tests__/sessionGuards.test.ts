import { isSocialCallbackPath, wasSentWithCurrentToken } from '../sessionGuards';

describe('isSocialCallbackPath', () => {
	it.each(['/login/google', '/login/github', '/login/apple', '/login/telegram?token=x'])(
		'returns true for the OAuth callback page %s',
		(path) => expect(isSocialCallbackPath(path)).toBe(true),
	);

	it.each(['/login', '/login/', '/', '/personal/list', '/register'])(
		'returns false for %s',
		(path) => expect(isSocialCallbackPath(path)).toBe(false),
	);
});

describe('wasSentWithCurrentToken', () => {
	it('is true when the request carried the token that is still current', () => {
		expect(wasSentWithCurrentToken('Bearer abc', 'abc')).toBe(true);
	});

	it('is true for a guest request while no token is set', () => {
		expect(wasSentWithCurrentToken(undefined, undefined)).toBe(true);
		expect(wasSentWithCurrentToken('', null)).toBe(true);
	});

	it('is false when the session rotated after the request left', () => {
		expect(wasSentWithCurrentToken('Bearer stale', 'fresh')).toBe(false);
	});

	it('is false when the request was sent as guest but a session exists now', () => {
		expect(wasSentWithCurrentToken(undefined, 'fresh')).toBe(false);
	});
});
