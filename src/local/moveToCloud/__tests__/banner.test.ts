import { copiedBannerText, formatBytes, previousMoveWarning } from '../banner';

describe('move banners', () => {
	it('words the keep-local banner', () => {
		expect(copiedBannerText({ name: 'Work', at: '2026-10-12T12:00:00Z' })).toBe(
			'Copied to your cloud workspace Work on Oct 12, 2026. Changes here are not synced.',
		);
	});

	it('words the previous move warning', () => {
		expect(previousMoveWarning({ at: '2026-10-01T12:00:00Z' })).toBe(
			'You moved this workspace on Oct 1, 2026. This creates a second cloud copy.',
		);
	});

	it('formats sizes', () => {
		expect(formatBytes(512)).toBe('512 B');
		expect(formatBytes(1536)).toBe('1.5 KB');
		expect(formatBytes(5 * 1024 * 1024)).toBe('5.0 MB');
	});
});
