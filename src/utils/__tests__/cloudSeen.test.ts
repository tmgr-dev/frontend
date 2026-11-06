import { CLOUD_SEEN_KEY, hasCloudSeen, hasRunBefore } from '../cloudSeen';

describe('hasCloudSeen', () => {
	it('is false on a storage without the marker and true with it', () => {
		const map = new Map<string, string>();
		const storage = { getItem: (key: string) => map.get(key) ?? null };
		expect(hasCloudSeen(storage)).toBe(false);
		map.set(CLOUD_SEEN_KEY, '1');
		expect(hasCloudSeen(storage)).toBe(true);
	});

	it('treats an unreadable storage as a cloud user so the demo never replaces Welcome', () => {
		expect(
			hasCloudSeen({
				getItem: () => {
					throw new Error('denied');
				},
			}),
		).toBe(true);
	});
});

describe('hasRunBefore', () => {
	it('is true once the app has recorded a seen version', () => {
		const map = new Map<string, string>();
		const storage = { getItem: (key: string) => map.get(key) ?? null };
		expect(hasRunBefore(storage)).toBe(false);
		map.set('desktop.whatsNew.lastSeenVersion', '0.9.107');
		expect(hasRunBefore(storage)).toBe(true);
	});
});
