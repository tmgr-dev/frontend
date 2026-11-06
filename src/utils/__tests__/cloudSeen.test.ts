import { CLOUD_SEEN_KEY, hasCloudSeen } from '../cloudSeen';

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
