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

const stored = (token: string, refresh: string) => () => ({
	token,
	refresh_token: refresh,
});

describe('createTokenRefresher', () => {
	it('returns null without calling post when storage holds no refresh token', async () => {
		const post = jest.fn();
		const refresh = createTokenRefresher({
			readStored: () => null,
			currentAccessToken: () => 'a',
			post,
			onSuccess: jest.fn(),
		});
		await expect(refresh()).resolves.toBeNull();
		expect(post).not.toHaveBeenCalled();
	});

	it('posts the refresh token from storage, not the one this tab remembers', async () => {
		const post = jest.fn().mockResolvedValue({ token: 'new-access', refresh_token: 'new-refresh' });
		const refresh = createTokenRefresher({
			readStored: stored('a', 'stored-refresh'),
			currentAccessToken: () => 'a',
			post,
			onSuccess: jest.fn(),
		});
		await expect(refresh()).resolves.toBe('new-access');
		expect(post).toHaveBeenCalledWith('stored-refresh');
	});

	it('resolves new access token and commits envelope on success', async () => {
		const envelope = { token: 'new-access', refresh_token: 'new-refresh' };
		const onSuccess = jest.fn();
		const refresh = createTokenRefresher({
			readStored: stored('a', 'old-refresh'),
			currentAccessToken: () => 'a',
			post: jest.fn().mockResolvedValue(envelope),
			onSuccess,
		});
		await expect(refresh()).resolves.toBe('new-access');
		expect(onSuccess).toHaveBeenCalledWith(envelope);
	});

	it('adopts a token another tab already rotated instead of posting', async () => {
		const post = jest.fn();
		const onSuccess = jest.fn();
		const refresh = createTokenRefresher({
			// storage moved on: another tab refreshed and persisted b/r2
			readStored: stored('b', 'r2'),
			currentAccessToken: () => 'a',
			post,
			onSuccess,
		});
		await expect(refresh()).resolves.toBe('b');
		expect(post).not.toHaveBeenCalled();
		expect(onSuccess).toHaveBeenCalledWith({ token: 'b', refresh_token: 'r2' });
	});

	it('runs the refresh inside the provided cross-tab lock', async () => {
		let lockAcquired = 0;
		const withLock = <T>(fn: () => Promise<T>) => {
			lockAcquired += 1;
			return fn();
		};
		const refresh = createTokenRefresher({
			readStored: stored('a', 'r'),
			currentAccessToken: () => 'a',
			post: jest.fn().mockResolvedValue({ token: 't' }),
			onSuccess: jest.fn(),
			withLock,
		});
		await expect(refresh()).resolves.toBe('t');
		expect(lockAcquired).toBe(1);
	});

	it('re-reads storage after acquiring the lock', async () => {
		// while this tab waited for the lock, the holder rotated the token
		let snapshot = { token: 'a', refresh_token: 'r1' };
		const withLock = <T>(fn: () => Promise<T>) => {
			snapshot = { token: 'b', refresh_token: 'r2' };
			return fn();
		};
		const post = jest.fn();
		const refresh = createTokenRefresher({
			readStored: () => snapshot,
			currentAccessToken: () => 'a',
			post,
			onSuccess: jest.fn(),
			withLock,
		});
		await expect(refresh()).resolves.toBe('b');
		expect(post).not.toHaveBeenCalled();
	});

	it('resolves null on malformed envelope without committing it', async () => {
		const onSuccess = jest.fn();
		const refresh = createTokenRefresher({
			readStored: stored('a', 'old-refresh'),
			currentAccessToken: () => 'a',
			post: jest.fn().mockResolvedValue(undefined as never),
			onSuccess,
		});
		await expect(refresh()).resolves.toBeNull();
		expect(onSuccess).not.toHaveBeenCalled();
	});

	it('resolves null on post failure without throwing', async () => {
		const refresh = createTokenRefresher({
			readStored: stored('a', 'old-refresh'),
			currentAccessToken: () => 'a',
			post: jest.fn().mockRejectedValue(new Error('401')),
			onSuccess: jest.fn(),
		});
		await expect(refresh()).resolves.toBeNull();
	});

	it('resolves null when the lock itself rejects', async () => {
		const refresh = createTokenRefresher({
			readStored: stored('a', 'r'),
			currentAccessToken: () => 'a',
			post: jest.fn(),
			onSuccess: jest.fn(),
			withLock: () => Promise.reject(new Error('lock unavailable')),
		});
		await expect(refresh()).resolves.toBeNull();
	});

	it('single-flight: concurrent calls share one post', async () => {
		let resolvePost!: (v: { token: string }) => void;
		const post = jest
			.fn()
			.mockReturnValue(new Promise((r) => (resolvePost = r)));
		const refresh = createTokenRefresher({
			readStored: stored('a', 'old-refresh'),
			currentAccessToken: () => 'a',
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
			readStored: stored('a', 'r'),
			currentAccessToken: () => 'a',
			post,
			onSuccess: jest.fn(),
		});
		await refresh();
		await refresh();
		expect(post).toHaveBeenCalledTimes(2);
	});
});
