import { invoke } from '@tauri-apps/api/core';
import {
	decideGitTrack,
	loadGitTrackFolders,
	loadGitTrackMode,
	MAX_GIT_TRACK_FOLDERS,
	saveGitTrackFolders,
	saveGitTrackMode,
	syncGitTrack,
} from '../gitTrack';

jest.mock('@tauri-apps/api/core', () => ({
	invoke: jest.fn(() => Promise.resolve()),
}));

const storage = new Map<string, string>();

beforeEach(() => {
	storage.clear();
	Object.defineProperty(globalThis, 'localStorage', {
		configurable: true,
		value: {
			getItem: (key: string) => (storage.has(key) ? storage.get(key)! : null),
			setItem: (key: string, value: string) => storage.set(key, String(value)),
			removeItem: (key: string) => storage.delete(key),
		},
	});
	(invoke as jest.Mock).mockClear();
});

describe('git track mode', () => {
	it('defaults to ask when nothing is stored', () => {
		expect(loadGitTrackMode()).toBe('ask');
	});

	it('persists the chosen mode', () => {
		saveGitTrackMode('auto');
		expect(storage.get('app.gitTrack.mode')).toBe('auto');
		expect(loadGitTrackMode()).toBe('auto');
	});

	it.each(['sometimes', '', 'AUTO', '"auto"'])(
		'falls back to ask for an invalid stored value %j',
		(value) => {
			storage.set('app.gitTrack.mode', value);
			expect(loadGitTrackMode()).toBe('ask');
		},
	);
});

describe('git track folders', () => {
	it('starts empty', () => {
		expect(loadGitTrackFolders()).toEqual([]);
	});

	it('persists the folder list in order', () => {
		saveGitTrackFolders(['/code/a', '/code/b']);
		expect(loadGitTrackFolders()).toEqual(['/code/a', '/code/b']);
	});

	it('caps the saved list at the maximum', () => {
		const folders = Array.from({ length: MAX_GIT_TRACK_FOLDERS + 5 }, (_, i) =>
			String(i),
		);
		saveGitTrackFolders(folders);
		expect(loadGitTrackFolders()).toHaveLength(MAX_GIT_TRACK_FOLDERS);
		expect(loadGitTrackFolders()[0]).toBe('0');
	});

	it('ignores a corrupt stored value', () => {
		storage.set('app.gitTrack.folders', '{not json');
		expect(loadGitTrackFolders()).toEqual([]);
	});

	it('keeps only string entries from a stored array', () => {
		storage.set('app.gitTrack.folders', JSON.stringify(['/a', 3, null, '/b']));
		expect(loadGitTrackFolders()).toEqual(['/a', '/b']);
	});
});

describe('decideGitTrack', () => {
	const task = { id: 157, workspaceId: 5, statusType: 'active' };
	const other = { id: 9, workspaceId: 5 };

	it('ignores everything when the mode is off', () => {
		expect(decideGitTrack({ mode: 'off', task, runningTasks: [] })).toBe(
			'ignore',
		);
	});

	it('ignores a missing task', () => {
		expect(decideGitTrack({ mode: 'ask', task: null, runningTasks: [] })).toBe(
			'ignore',
		);
		expect(
			decideGitTrack({ mode: 'auto', task: undefined, runningTasks: [] }),
		).toBe('ignore');
	});

	it('ignores a task whose timer already runs', () => {
		const running = [{ id: 157, workspace_id: 5 }];
		expect(decideGitTrack({ mode: 'ask', task, runningTasks: running })).toBe(
			'ignore',
		);
		expect(decideGitTrack({ mode: 'auto', task, runningTasks: running })).toBe(
			'ignore',
		);
	});

	it('does not confuse a running task with the same id in another workspace', () => {
		const running = [{ id: 157, workspace_id: 6 }];
		expect(decideGitTrack({ mode: 'auto', task, runningTasks: running })).toBe(
			'switch',
		);
	});

	it('asks in ask mode, also when another timer runs', () => {
		expect(decideGitTrack({ mode: 'ask', task, runningTasks: [] })).toBe('ask');
		expect(decideGitTrack({ mode: 'ask', task, runningTasks: [other] })).toBe(
			'ask',
		);
	});

	it('switches in auto mode', () => {
		expect(decideGitTrack({ mode: 'auto', task, runningTasks: [other] })).toBe(
			'switch',
		);
	});

	it.each(['completed', 'archived'])(
		'ignores a %s task in auto mode',
		(statusType) => {
			expect(
				decideGitTrack({
					mode: 'auto',
					task: { ...task, statusType },
					runningTasks: [],
				}),
			).toBe('ignore');
		},
	);
});

describe('syncGitTrack', () => {
	it('sends an empty list when the mode is off', async () => {
		saveGitTrackMode('off');
		saveGitTrackFolders(['/code/a']);
		await syncGitTrack();
		expect(invoke).toHaveBeenCalledWith('git_track_update', { folders: [] });
	});

	it('sends the watched folders when the mode is ask or auto', async () => {
		saveGitTrackFolders(['/code/a', '/code/b']);
		await syncGitTrack();
		expect(invoke).toHaveBeenLastCalledWith('git_track_update', {
			folders: ['/code/a', '/code/b'],
		});
		saveGitTrackMode('auto');
		await syncGitTrack();
		expect(invoke).toHaveBeenLastCalledWith('git_track_update', {
			folders: ['/code/a', '/code/b'],
		});
	});
});
