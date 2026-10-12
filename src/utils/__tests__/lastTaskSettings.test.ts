import {
	applicableTaskSettings,
	hasTaskSettings,
	loadLastTaskSettings,
	saveLastTaskSettings,
	type LastTaskSettings,
} from '../lastTaskSettings';

const memoryStorage = () => {
	const data = new Map<string, string>();
	return {
		data,
		getItem: (key: string) => data.get(key) ?? null,
		setItem: (key: string, value: string) => void data.set(key, value),
	};
};

const persona = {
	id: 'p-1',
	name: 'Reviewer',
	avatar_url: null,
	owner: { id: 7, name: 'Ann' },
	workspace_id: 1,
};

const settings: LastTaskSettings = {
	status_id: 3,
	project_category_id: 12,
	assignees: [7, 9],
	persona_assignees: [persona],
	approximately_time: 3600,
};

describe('lastTaskSettings', () => {
	it('round-trips settings for a workspace', () => {
		const storage = memoryStorage();
		saveLastTaskSettings('acme', settings, storage);
		expect(loadLastTaskSettings('acme', storage)).toEqual(settings);
	});

	it('returns null when nothing is saved', () => {
		expect(loadLastTaskSettings('acme', memoryStorage())).toBeNull();
	});

	it('keeps workspaces apart', () => {
		const storage = memoryStorage();
		saveLastTaskSettings('acme', settings, storage);
		expect(loadLastTaskSettings('other', storage)).toBeNull();
	});

	it('returns null for corrupt JSON and non-objects', () => {
		const storage = memoryStorage();
		storage.data.set('tmgr:last-task-settings:acme', '{oops');
		expect(loadLastTaskSettings('acme', storage)).toBeNull();
		storage.data.set('tmgr:last-task-settings:acme', '42');
		expect(loadLastTaskSettings('acme', storage)).toBeNull();
	});

	it('drops malformed fields', () => {
		const storage = memoryStorage();
		storage.data.set(
			'tmgr:last-task-settings:acme',
			JSON.stringify({
				status_id: 'x',
				project_category_id: -1,
				assignees: [1, 'two', null],
				persona_assignees: [persona, { name: 'no id' }, null],
				approximately_time: 'NaN',
			}),
		);
		expect(loadLastTaskSettings('acme', storage)).toEqual({
			status_id: null,
			project_category_id: null,
			assignees: [1],
			persona_assignees: [persona],
			approximately_time: 0,
		});
	});

	it('does nothing without a workspace code or storage', () => {
		const storage = memoryStorage();
		saveLastTaskSettings('', settings, storage);
		expect(storage.data.size).toBe(0);
		expect(loadLastTaskSettings('acme', null)).toBeNull();
	});

	it('survives a throwing storage', () => {
		const storage = {
			getItem: () => null,
			setItem: () => {
				throw new Error('quota');
			},
		};
		expect(() => saveLastTaskSettings('acme', settings, storage)).not.toThrow();
	});

	it('drops ids that no longer exist', () => {
		expect(
			applicableTaskSettings(settings, {
				statusIds: [1, 2],
				categoryIds: [12],
				memberIds: [9],
			}),
		).toEqual({
			...settings,
			status_id: null,
			project_category_id: 12,
			assignees: [9],
		});
	});

	it('knows when there is nothing to apply', () => {
		expect(hasTaskSettings(null)).toBe(false);
		expect(
			hasTaskSettings({
				status_id: null,
				project_category_id: null,
				assignees: [],
				persona_assignees: [],
				approximately_time: 0,
			}),
		).toBe(false);
		expect(hasTaskSettings(settings)).toBe(true);
	});
});
