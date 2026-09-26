import {
	mergeWorkspaces,
	requestedWorkspace,
	withCurrentWorkspace,
	withoutLocalWorkspace,
} from '../patch';

const local = {
	id: -42,
	name: 'Personal',
	code: 'personal',
	schema_version: 1,
	created_at: '',
	path: '/Users/me/.tmgr.dev/workspaces/personal',
	database: '/Users/me/.tmgr.dev/workspaces/personal/workspace.db',
};

const user = {
	id: 7,
	settings: [
		{ id: 3, key: 'theme', value: 'dark' },
		{ id: 5, key: 'current_workspace', value: 56 },
	],
};

it('appends local workspaces shaped like cloud ones with a prefixed code', () => {
	const merged = mergeWorkspaces([{ id: 56, code: 'tmgrdev' }], [local], 7);
	expect(merged.map((w) => [w.id, w.code])).toEqual([
		[56, 'tmgrdev'],
		[-42, 'local-personal'],
	]);
	expect(merged[1]).toMatchObject({ type: 'local', is_local: true, user_id: 7 });
});

it('does not duplicate local workspaces when the list is merged twice', () => {
	const once = mergeWorkspaces([{ id: 56 }], [local], 7);
	expect(mergeWorkspaces(once, [local], 7)).toHaveLength(2);
});

it('points current_workspace at the local workspace without touching other settings', () => {
	const patched = withCurrentWorkspace(user, -42);
	expect(patched.settings).toEqual([
		{ id: 3, key: 'theme', value: 'dark' },
		{ id: 5, key: 'current_workspace', value: -42 },
	]);
	expect(user.settings[1].value).toBe(56);
});

it('reads the workspace a settings update switches to', () => {
	const payload = [
		{ id: 3, value: 'dark' },
		{ id: 5, value: -42 },
	];
	expect(requestedWorkspace(payload, user)).toBe(-42);
	expect(requestedWorkspace([{ id: 3, value: 'x' }], user)).toBeNull();
});

it('keeps the server current workspace when the payload names a local one', () => {
	const payload = [
		{ id: 3, value: 'light' },
		{ id: 5, value: -42 },
	];
	expect(withoutLocalWorkspace(payload, user, 56)).toEqual([
		{ id: 3, value: 'light' },
		{ id: 5, value: 56 },
	]);
});
