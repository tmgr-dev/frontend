import { refreshAction, shouldShowUpdateBanner } from '../realtime';

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

describe('refreshAction', () => {
	const draft = (body: string) => ({ title: 'T', body, properties: {} });

	it('own echo before the PATCH response looks foreign, so the decision must wait for the save', () => {
		expect(
			refreshAction({
				serverVersion: 11,
				loadedVersion: 10,
				ownVersions: new Set([10]),
				form: draft('abc'),
				saved: draft('ab'),
			}),
		).toBe('conflict');
		expect(
			shouldShowUpdateBanner({
				pageId: 1,
				loadedVersion: 10,
				ownVersions: new Set([10]),
				event: event(1, 11),
			}),
		).toBe(11);
	});

	it('own echo once the save settled is ignored', () => {
		expect(
			refreshAction({
				serverVersion: 11,
				loadedVersion: 11,
				ownVersions: new Set([10, 11]),
				form: draft('abc'),
				saved: draft('abc'),
			}),
		).toBe('ignore');
		expect(
			shouldShowUpdateBanner({
				pageId: 1,
				loadedVersion: 11,
				ownVersions: new Set([10, 11]),
				event: event(1, 11),
			}),
		).toBeNull();
	});

	it('keeps the draft typed after the save snapshot on an own version', () => {
		expect(
			refreshAction({
				serverVersion: 11,
				loadedVersion: 11,
				ownVersions: new Set([11]),
				form: draft('abcd'),
				saved: draft('abc'),
			}),
		).toBe('ignore');
	});

	it('an own version newer than the loaded one is ignored', () => {
		expect(
			refreshAction({
				serverVersion: 12,
				loadedVersion: 11,
				ownVersions: new Set([12]),
				form: draft('x'),
				saved: draft('y'),
			}),
		).toBe('ignore');
	});

	it('foreign change while dirty is a conflict', () => {
		expect(
			refreshAction({
				serverVersion: 12,
				loadedVersion: 11,
				ownVersions: new Set([11]),
				form: draft('mine'),
				saved: draft('base'),
			}),
		).toBe('conflict');
	});

	it('foreign change while clean is adopted', () => {
		expect(
			refreshAction({
				serverVersion: 12,
				loadedVersion: 11,
				ownVersions: new Set([11]),
				form: draft('base'),
				saved: draft('base'),
			}),
		).toBe('adopt');
	});
});
