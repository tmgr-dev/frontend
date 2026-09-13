import {
	absoluteLinkUrl,
	createSignedLinkCache,
	type SignedLink,
} from '@/utils/signedFileLinks';

const link = (url: string, expiresInMs: number, now: number): SignedLink => ({
	url,
	expiresAt: now + expiresInMs,
});

describe('absoluteLinkUrl', () => {
	it('resolves the API-relative link against the API origin', () => {
		expect(
			absoluteLinkUrl(
				'/api/files/44/content?uid=74&exp=1&sig=ab',
				'https://api.tmgr.dev/api/',
			),
		).toBe('https://api.tmgr.dev/api/files/44/content?uid=74&exp=1&sig=ab');
	});

	it('keeps an absolute link as it is', () => {
		expect(
			absoluteLinkUrl(
				'https://cdn.example.com/x.png',
				'https://api.tmgr.dev/api/',
			),
		).toBe('https://cdn.example.com/x.png');
	});
});

describe('createSignedLinkCache', () => {
	it('signs once and reuses the link while it is fresh', async () => {
		let now = 1_000_000;
		let calls = 0;
		const cache = createSignedLinkCache({
			sign: async (id) => {
				calls += 1;

				return link(`/api/files/${id}/content?sig=${calls}`, 3_600_000, now);
			},
			now: () => now,
		});

		expect(await cache.get(44)).toBe('/api/files/44/content?sig=1');
		now += 60_000;
		expect(await cache.get(44)).toBe('/api/files/44/content?sig=1');
		expect(calls).toBe(1);
	});

	it('re-signs once the link is inside the renewal margin', async () => {
		let now = 1_000_000;
		let calls = 0;
		const cache = createSignedLinkCache({
			sign: async (id) => {
				calls += 1;

				return link(`/api/files/${id}/content?sig=${calls}`, 600_000, now);
			},
			now: () => now,
		});

		await cache.get(44);
		now += 599_000;
		expect(await cache.get(44)).toBe('/api/files/44/content?sig=2');
		expect(calls).toBe(2);
	});

	it('coalesces concurrent requests for the same file onto one signature', async () => {
		let calls = 0;
		const cache = createSignedLinkCache({
			sign: async (id) => {
				calls += 1;
				await Promise.resolve();

				return link(`/api/files/${id}/content?sig=${calls}`, 3_600_000, 0);
			},
			now: () => 0,
		});

		const [a, b] = await Promise.all([cache.get(7), cache.get(7)]);

		expect(a).toBe(b);
		expect(calls).toBe(1);
	});

	it('stops asking for the rest of the session when signing is unavailable', async () => {
		let calls = 0;
		const cache = createSignedLinkCache({
			sign: async () => {
				calls += 1;

				return null;
			},
			now: () => 0,
		});

		expect(await cache.get(1)).toBeNull();
		expect(await cache.get(2)).toBeNull();
		expect(calls).toBe(1);
	});

	it('does not disable signing when one file simply fails', async () => {
		let calls = 0;
		const cache = createSignedLinkCache({
			sign: async (id) => {
				calls += 1;

				if (id === 1) {
					throw new Error('403');
				}

				return link(`/api/files/${id}/content?sig=ok`, 3_600_000, 0);
			},
			now: () => 0,
		});

		await expect(cache.get(1)).rejects.toThrow('403');
		expect(await cache.get(2)).toBe('/api/files/2/content?sig=ok');
		expect(calls).toBe(2);
	});

	it('forgets a file when it is invalidated', async () => {
		let calls = 0;
		const cache = createSignedLinkCache({
			sign: async (id) => {
				calls += 1;

				return link(`/api/files/${id}/content?sig=${calls}`, 3_600_000, 0);
			},
			now: () => 0,
		});

		await cache.get(44);
		cache.invalidate(44);

		expect(await cache.get(44)).toBe('/api/files/44/content?sig=2');
	});
});
