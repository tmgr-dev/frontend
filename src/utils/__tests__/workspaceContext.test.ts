import {
	isKnownWorkspaceId,
	overlayCurrentWorkspace,
	readWorkspaceId,
	resolveWorkspaceId,
	shouldAttachWorkspaceHeader,
	withoutCurrentWorkspaceEntry,
	writeWorkspaceId,
} from '@/utils/workspaceContext';

const memoryStorage = () => {
	const map = new Map<string, string>();
	return {
		getItem: (key: string) => (map.has(key) ? map.get(key)! : null),
		setItem: (key: string, value: string) => void map.set(key, value),
		removeItem: (key: string) => void map.delete(key),
	};
};

describe('readWorkspaceId / writeWorkspaceId', () => {
	test('round-trips a numeric id', () => {
		const storage = memoryStorage();
		writeWorkspaceId(storage, 'k', 5);
		expect(readWorkspaceId(storage, 'k')).toBe(5);
	});

	test('removes the key when writing null', () => {
		const storage = memoryStorage();
		writeWorkspaceId(storage, 'k', 5);
		writeWorkspaceId(storage, 'k', null);
		expect(readWorkspaceId(storage, 'k')).toBeNull();
	});

	test('returns null for a missing key, missing storage, or a throwing storage', () => {
		expect(readWorkspaceId(memoryStorage(), 'missing')).toBeNull();
		expect(readWorkspaceId(undefined, 'k')).toBeNull();
		const throwing = {
			getItem: () => {
				throw new Error('blocked');
			},
			setItem: () => {},
			removeItem: () => {},
		};
		expect(readWorkspaceId(throwing, 'k')).toBeNull();
		expect(() => writeWorkspaceId(throwing, 'k', 1)).not.toThrow();
	});

	test('negative ids (local workspaces) round-trip too', () => {
		const storage = memoryStorage();
		writeWorkspaceId(storage, 'k', -3);
		expect(readWorkspaceId(storage, 'k')).toBe(-3);
	});
});

describe('isKnownWorkspaceId', () => {
	const workspaces = [{ id: 1 }, { id: '2' }];
	test('matches by loose numeric equality', () => {
		expect(isKnownWorkspaceId(1, workspaces)).toBe(true);
		expect(isKnownWorkspaceId(2, workspaces)).toBe(true);
	});
	test('rejects an id not in the list, null, or a missing list', () => {
		expect(isKnownWorkspaceId(3, workspaces)).toBe(false);
		expect(isKnownWorkspaceId(null, workspaces)).toBe(false);
		expect(isKnownWorkspaceId(1, undefined)).toBe(false);
	});
});

describe('resolveWorkspaceId', () => {
	const workspaces = [{ id: 1 }, { id: 2 }];

	test('prefers the URL over everything else', () => {
		expect(
			resolveWorkspaceId({
				urlWorkspaceId: 2,
				sessionWorkspaceId: 1,
				lastWorkspaceId: 1,
				defaultWorkspaceId: 1,
				workspaces,
			}),
		).toBe(2);
	});

	test('falls back to sessionStorage, then localStorage, then the server default', () => {
		expect(
			resolveWorkspaceId({
				sessionWorkspaceId: 2,
				defaultWorkspaceId: 1,
				workspaces,
			}),
		).toBe(2);
		expect(
			resolveWorkspaceId({
				lastWorkspaceId: 2,
				defaultWorkspaceId: 1,
				workspaces,
			}),
		).toBe(2);
		expect(resolveWorkspaceId({ defaultWorkspaceId: 1, workspaces })).toBe(1);
	});

	test('skips a candidate the loaded workspace list does not recognise', () => {
		expect(
			resolveWorkspaceId({
				urlWorkspaceId: 99,
				sessionWorkspaceId: 2,
				workspaces,
			}),
		).toBe(2);
	});

	test('trusts a candidate provisionally when the workspace list has not loaded yet', () => {
		expect(resolveWorkspaceId({ sessionWorkspaceId: 99, workspaces: [] })).toBe(
			99,
		);
	});

	test('returns null when nothing resolves', () => {
		expect(resolveWorkspaceId({ workspaces })).toBeNull();
	});
});

describe('shouldAttachWorkspaceHeader', () => {
	const workspaces = [{ id: 1 }, { id: -3 }];

	test('true for a known, positive cloud workspace id', () => {
		expect(shouldAttachWorkspaceHeader(1, workspaces)).toBe(true);
	});

	test('false for a negative (local) id, even if technically in the list', () => {
		expect(shouldAttachWorkspaceHeader(-3, workspaces)).toBe(false);
	});

	test('false when missing, unloaded, or unknown', () => {
		expect(shouldAttachWorkspaceHeader(null, workspaces)).toBe(false);
		expect(shouldAttachWorkspaceHeader(1, [])).toBe(false);
		expect(shouldAttachWorkspaceHeader(7, workspaces)).toBe(false);
	});
});

describe('overlayCurrentWorkspace', () => {
	test('replaces the existing entry value', () => {
		const settings = [{ id: 5, key: 'current_workspace', value: 1 }];
		expect(overlayCurrentWorkspace(settings, 2)).toEqual([
			{ id: 5, key: 'current_workspace', value: 2 },
		]);
	});

	test('appends the entry when absent', () => {
		const settings = [{ id: 1, key: 'theme', value: 'dark' }];
		expect(overlayCurrentWorkspace(settings, 2)).toEqual([
			{ id: 1, key: 'theme', value: 'dark' },
			{ key: 'current_workspace', value: 2 },
		]);
	});

	test('is a no-op without settings or without a workspace id', () => {
		expect(overlayCurrentWorkspace(undefined, 2)).toBeUndefined();
		const settings = [{ id: 5, key: 'current_workspace', value: 1 }];
		expect(overlayCurrentWorkspace(settings, null)).toBe(settings);
	});
});

describe('withoutCurrentWorkspaceEntry', () => {
	const payload = [
		{ id: 5, value: 9 },
		{ id: 6, value: 'dark' },
	];

	test('drops the current_workspace entry', () => {
		expect(withoutCurrentWorkspaceEntry(payload, 5)).toEqual([
			{ id: 6, value: 'dark' },
		]);
	});

	test('leaves the payload untouched when the caller explicitly sets the default', () => {
		expect(withoutCurrentWorkspaceEntry(payload, 5, true)).toBe(payload);
	});

	test('is a no-op when the setting id is unknown or the payload is not an array', () => {
		expect(withoutCurrentWorkspaceEntry(payload, null)).toBe(payload);
		expect(withoutCurrentWorkspaceEntry({ settings: [] }, 5)).toEqual({
			settings: [],
		});
	});

	test('leaves a negative (local workspace) target untouched, even without the explicit flag', () => {
		const localSwitch = [{ id: 5, value: -3 }];
		expect(withoutCurrentWorkspaceEntry(localSwitch, 5)).toEqual([
			{ id: 5, value: -3 },
		]);
	});
});
