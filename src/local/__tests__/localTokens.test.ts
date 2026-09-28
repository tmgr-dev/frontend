const invoke = jest.fn();

jest.mock('@tauri-apps/api/core', () => ({ invoke }));

const load = () => {
	let mod!: typeof import('../localTokens');
	jest.isolateModules(() => {
		mod = require('../localTokens');
	});
	return mod;
};

beforeEach(() => {
	invoke.mockReset();
});

it('issues a token with the exact command and args', async () => {
	const token = { id: 'lt_1', prefix: 'tmgrl_abcd' };
	invoke.mockResolvedValue(token);
	const { issueLocalToken } = load();

	const args = {
		personaUuid: 'p-1',
		personaName: 'Rex',
		workspaceCode: 'local-1',
		label: 'Claude on laptop',
		expiresInDays: 90,
	};
	const result = await issueLocalToken(args);

	expect(invoke).toHaveBeenCalledWith('local_token_issue', args);
	expect(result).toBe(token);
});

it('lists tokens scoped to a workspace code', async () => {
	invoke.mockResolvedValue([]);
	const { listLocalTokens } = load();

	await listLocalTokens('local-1');

	expect(invoke).toHaveBeenCalledWith('local_token_list', { workspaceCode: 'local-1' });
});

it('revokes a single token by id', async () => {
	invoke.mockResolvedValue(undefined);
	const { revokeLocalToken } = load();

	await revokeLocalToken('lt_1');

	expect(invoke).toHaveBeenCalledWith('local_token_revoke', { id: 'lt_1' });
});

it('revokes all tokens matching a filter', async () => {
	invoke.mockResolvedValue(3);
	const { revokeAllLocalTokens } = load();

	const result = await revokeAllLocalTokens({ personaUuid: 'p-1' });

	expect(invoke).toHaveBeenCalledWith('local_token_revoke_all', { personaUuid: 'p-1' });
	expect(result).toBe(3);
});

it('copies a token to the clipboard without returning a secret', async () => {
	invoke.mockResolvedValue(undefined);
	const { copyLocalToken } = load();

	const result = await copyLocalToken('lt_1');

	expect(invoke).toHaveBeenCalledWith('local_token_copy', { id: 'lt_1' });
	expect(result).toBeUndefined();
});

it('reads local access status', async () => {
	const status = {
		enabled: true,
		listening: true,
		socketPath: '/tmp/local-access.sock',
		safeMode: false,
		ready: true,
		bridgeCommand: '/Applications/TMGR.app/Contents/MacOS/TMGR',
	};
	invoke.mockResolvedValue(status);
	const { getLocalAccessStatus } = load();

	const result = await getLocalAccessStatus();

	expect(invoke).toHaveBeenCalledWith('local_access_status', undefined);
	expect(result).toBe(status);
});

it('toggles the kill switch', async () => {
	invoke.mockResolvedValue(undefined);
	const { setLocalAccessEnabled } = load();

	await setLocalAccessEnabled(false);

	expect(invoke).toHaveBeenCalledWith('local_access_set_enabled', { enabled: false });
});
