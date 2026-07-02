import { createTokenRefresher, isAuthUrl } from '../tokenRefresher';

describe('isAuthUrl', () => {
	it.each([
		'auth/login',
		'/auth/login',
		'auth/login/github/redirect',
		'auth/register',
		'auth/refresh',
	])('returns true for %s', (url) => expect(isAuthUrl(url)).toBe(true));

	it.each(['auth/logout', 'tasks/1', 'workspaces', undefined])(
		'returns false for %s',
		(url) => expect(isAuthUrl(url as string | undefined)).toBe(false),
	);
});

describe('createTokenRefresher', () => {
	it('returns null without calling post when no refresh token', async () => {
		const post = jest.fn();
		const refresh = createTokenRefresher({
			getRefreshToken: () => undefined,
			post,
			onSuccess: jest.fn(),
		});
		await expect(refresh()).resolves.toBeNull();
		expect(post).not.toHaveBeenCalled();
	});

	it('resolves new access token and commits envelope on success', async () => {
		const envelope = { token: 'new-access', refresh_token: 'new-refresh' };
		const onSuccess = jest.fn();
		const refresh = createTokenRefresher({
			getRefreshToken: () => 'old-refresh',
			post: jest.fn().mockResolvedValue(envelope),
			onSuccess,
		});
		await expect(refresh()).resolves.toBe('new-access');
		expect(onSuccess).toHaveBeenCalledWith(envelope);
	});

	it('resolves null on malformed envelope without committing it', async () => {
		const onSuccess = jest.fn();
		const refresh = createTokenRefresher({
			getRefreshToken: () => 'old-refresh',
			post: jest.fn().mockResolvedValue(undefined as never),
			onSuccess,
		});
		await expect(refresh()).resolves.toBeNull();
		expect(onSuccess).not.toHaveBeenCalled();
	});

	it('resolves null on post failure without throwing', async () => {
		const refresh = createTokenRefresher({
			getRefreshToken: () => 'old-refresh',
			post: jest.fn().mockRejectedValue(new Error('401')),
			onSuccess: jest.fn(),
		});
		await expect(refresh()).resolves.toBeNull();
	});

	it('single-flight: concurrent calls share one post', async () => {
		let resolvePost!: (v: { token: string }) => void;
		const post = jest
			.fn()
			.mockReturnValue(new Promise((r) => (resolvePost = r)));
		const refresh = createTokenRefresher({
			getRefreshToken: () => 'old-refresh',
			post,
			onSuccess: jest.fn(),
		});
		const p1 = refresh();
		const p2 = refresh();
		resolvePost({ token: 'fresh' });
		await expect(Promise.all([p1, p2])).resolves.toEqual(['fresh', 'fresh']);
		expect(post).toHaveBeenCalledTimes(1);
	});

	it('allows a new refresh after the previous one settles', async () => {
		const post = jest.fn().mockResolvedValue({ token: 't' });
		const refresh = createTokenRefresher({
			getRefreshToken: () => 'r',
			post,
			onSuccess: jest.fn(),
		});
		await refresh();
		await refresh();
		expect(post).toHaveBeenCalledTimes(2);
	});
});
