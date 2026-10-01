import { shouldShowUpdateBanner } from '../realtime';

const event = (id: number, version: number) => ({ page: { id, version } });

describe('shouldShowUpdateBanner', () => {
	const base = { pageId: 1, loadedVersion: 3, ownVersions: new Set<number>() };

	it('shows for a newer foreign version', () => {
		expect(shouldShowUpdateBanner({ ...base, event: event(1, 4) })).toBe(4);
	});

	it('ignores other pages', () => {
		expect(shouldShowUpdateBanner({ ...base, event: event(2, 9) })).toBeNull();
	});

	it('ignores same or older versions', () => {
		expect(shouldShowUpdateBanner({ ...base, event: event(1, 3) })).toBeNull();
		expect(shouldShowUpdateBanner({ ...base, event: event(1, 2) })).toBeNull();
	});

	it('ignores my own saves', () => {
		expect(
			shouldShowUpdateBanner({
				...base,
				ownVersions: new Set([4]),
				event: event(1, 4),
			}),
		).toBeNull();
	});

	it('ignores malformed events', () => {
		expect(shouldShowUpdateBanner({ ...base, event: undefined })).toBeNull();
		expect(
			shouldShowUpdateBanner({ ...base, event: { page: undefined } as any }),
		).toBeNull();
	});

	it('a newer foreign version still shows when an older own version exists', () => {
		expect(
			shouldShowUpdateBanner({
				...base,
				ownVersions: new Set([4]),
				event: event(1, 5),
			}),
		).toBe(5);
	});
});
