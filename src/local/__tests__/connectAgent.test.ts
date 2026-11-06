const invoke = jest.fn();
jest.mock('@tauri-apps/api/core', () => ({
	invoke: (...a: any[]) => invoke(...a),
}));

const calls: string[] = [];
const ctx = { id: 'ctx' } as any;
const localContext = jest.fn(async () => ctx);
jest.mock('@/local/runtime', () => ({
	localContext: (...a: any[]) => (localContext as any)(...a),
}));

const writeFeatures = jest.fn(async () => {
	calls.push('writeFeatures');
});
const localModulesPayload = jest.fn();
jest.mock('@/local/modulesApi', () => ({
	writeFeatures: (...a: any[]) => (writeFeatures as any)(...a),
	localModulesPayload: (...a: any[]) => localModulesPayload(...a),
}));

const ensureDefaultLocalPersona = jest.fn(async () => {
	calls.push('ensurePersona');
	return { uuid: 'p1' };
});
const listLocalPersonas = jest.fn();
const enableLocalPersona = jest.fn(async () => {
	calls.push('enablePersona');
});
jest.mock('@/local/personas', () => ({
	ensureDefaultLocalPersona: (...a: any[]) =>
		(ensureDefaultLocalPersona as any)(...a),
	listLocalPersonas: (...a: any[]) => listLocalPersonas(...a),
	enableLocalPersona: (...a: any[]) => (enableLocalPersona as any)(...a),
}));

const issueLocalToken = jest.fn();
const listLocalTokens = jest.fn();
const revokeLocalToken = jest.fn(async (id: string) => {
	calls.push(`revoke:${id}`);
});
const getLocalAccessStatus = jest.fn();
jest.mock('@/local/localTokens', () => ({
	issueLocalToken: (...a: any[]) => issueLocalToken(...a),
	listLocalTokens: (...a: any[]) => listLocalTokens(...a),
	revokeLocalToken: (...a: any[]) => (revokeLocalToken as any)(...a),
	getLocalAccessStatus: (...a: any[]) => getLocalAccessStatus(...a),
}));

import {
	CONNECT_AGENTS,
	configSnippet,
	copyConfigToken,
	detectAgents,
	installAgent,
	removeAgent,
	runAgentConnect,
} from '@/local/connectAgent';

const workspace = { id: 1, name: 'W', code: 'w1', demo: false } as any;
const user = { id: -1, name: 'Me', email: '' };
const base = { agent: 'claude-code' as const, workspace, user };
const LABEL = 'Claude Code (Connect screen)';

const tokenRow = (over: Record<string, unknown> = {}) => ({
	id: 'lt_aaaaaaaaaaaaaaaa',
	label: LABEL,
	workspaceCode: 'w1',
	revokedAt: null,
	pluginId: null,
	expiresAt: new Date(Date.now() + 86400000).toISOString(),
	...over,
});

const okRun = { ok: true, timedOut: false, steps: [] };

beforeEach(() => {
	jest.clearAllMocks();
	calls.length = 0;
	localModulesPayload.mockResolvedValue({
		modules: [
			{ key: 'personas', enabled: false },
			{ key: 'agent_work', enabled: false },
		],
	});
	listLocalPersonas.mockResolvedValue([
		{
			uuid: 'p1',
			name: 'Local agent',
			permissions: ['tasks:read'],
			disabled_at: null,
		},
	]);
	issueLocalToken.mockImplementation(async () => {
		calls.push('issue');
		return tokenRow({ id: 'lt_newnewnewnewnewn' });
	});
	listLocalTokens.mockResolvedValue([]);
	invoke.mockImplementation(async (cmd: string) => {
		calls.push(`invoke:${cmd}`);
		return okRun;
	});
});

describe('CONNECT_AGENTS', () => {
	it('describes the three agents with short labels', () => {
		expect(CONNECT_AGENTS.map((a) => a.id)).toEqual([
			'claude-code',
			'codex',
			'cursor',
		]);
		expect(CONNECT_AGENTS.map((a) => a.canInstall)).toEqual([
			true,
			true,
			false,
		]);
		expect(CONNECT_AGENTS.map((a) => a.tokenLabel)).toEqual([
			'Claude Code (Connect screen)',
			'Codex (Connect screen)',
			'Cursor (Connect screen)',
		]);
		for (const a of CONNECT_AGENTS)
			expect(a.tokenLabel.length).toBeLessThanOrEqual(60);
	});
});

describe('wrappers', () => {
	it('detectAgents calls agents_detect', async () => {
		invoke.mockResolvedValueOnce({ agents: [] });
		await detectAgents();
		expect(invoke).toHaveBeenCalledWith('agents_detect', undefined);
	});

	it('runAgentConnect passes agent, action and token id', async () => {
		await runAgentConnect('codex', 'install', 'lt_x');
		expect(invoke).toHaveBeenCalledWith('agent_connect_run', {
			agent: 'codex',
			action: 'install',
			tokenId: 'lt_x',
		});
	});
});

describe('installAgent', () => {
	it('runs modules, persona, token, install in order', async () => {
		const result = await installAgent(base);
		expect(calls).toEqual([
			'writeFeatures',
			'ensurePersona',
			'issue',
			'invoke:agent_connect_run',
		]);
		expect(writeFeatures).toHaveBeenCalledWith(ctx, {
			personas: true,
			agent_work: true,
		});
		expect(localContext).toHaveBeenCalledWith(workspace, user);
		expect(issueLocalToken).toHaveBeenCalledWith({
			personaUuid: 'p1',
			personaName: 'Local agent',
			workspaceCode: 'w1',
			label: LABEL,
			expiresInDays: 90,
		});
		expect(invoke).toHaveBeenLastCalledWith('agent_connect_run', {
			agent: 'claude-code',
			action: 'install',
			tokenId: 'lt_newnewnewnewnewn',
		});
		expect(result).toMatchObject({ status: 'ok', modulesChanged: true });
	});

	it('refuses the demo workspace without side effects', async () => {
		const result = await installAgent({
			...base,
			workspace: { ...workspace, demo: true },
		});
		expect(result.status).toBe('failed');
		expect(result.error).toMatch(/own workspace/i);
		expect(issueLocalToken).not.toHaveBeenCalled();
		expect(invoke).not.toHaveBeenCalled();
	});

	it('leaves modules alone when they are already on', async () => {
		localModulesPayload.mockResolvedValue({
			modules: [
				{ key: 'personas', enabled: true },
				{ key: 'agent_work', enabled: true },
			],
		});
		const result = await installAgent(base);
		expect(writeFeatures).not.toHaveBeenCalled();
		expect(result.modulesChanged).toBe(false);
	});

	it('writes only the module that is off', async () => {
		localModulesPayload.mockResolvedValue({
			modules: [
				{ key: 'personas', enabled: true },
				{ key: 'agent_work', enabled: false },
			],
		});
		await installAgent(base);
		expect(writeFeatures).toHaveBeenCalledWith(ctx, { agent_work: true });
	});

	it('re-enables a disabled persona', async () => {
		listLocalPersonas.mockResolvedValue([
			{
				uuid: 'p1',
				name: 'Local agent',
				permissions: ['tasks:read'],
				disabled_at: '2026-01-01',
			},
		]);
		await installAgent(base);
		expect(enableLocalPersona).toHaveBeenCalledWith(ctx, 'p1', ['tasks:read']);
		expect(calls.indexOf('enablePersona')).toBeLessThan(calls.indexOf('issue'));
	});

	it('does not re-enable an enabled persona', async () => {
		await installAgent(base);
		expect(enableLocalPersona).not.toHaveBeenCalled();
	});

	it('revokes the new token on a clean failure and keeps old ones', async () => {
		invoke.mockResolvedValueOnce({
			ok: false,
			timedOut: false,
			steps: [
				{ display: ['x'], exitCode: 1, output: 'boom', tolerated: false },
			],
		});
		listLocalTokens.mockResolvedValue([tokenRow()]);
		const result = await installAgent(base);
		expect(result.status).toBe('failed');
		expect(result.outcome?.steps[0].output).toBe('boom');
		expect(revokeLocalToken).toHaveBeenCalledTimes(1);
		expect(revokeLocalToken).toHaveBeenCalledWith('lt_newnewnewnewnewn');
	});

	it('revokes the new token when the command could not start', async () => {
		invoke.mockRejectedValueOnce('CLI not found');
		const result = await installAgent(base);
		expect(result).toMatchObject({ status: 'failed', error: 'CLI not found' });
		expect(revokeLocalToken).toHaveBeenCalledWith('lt_newnewnewnewnewn');
	});

	it('keeps the token on timeout', async () => {
		invoke.mockResolvedValueOnce({ ok: false, timedOut: true, steps: [] });
		const result = await installAgent(base);
		expect(result.status).toBe('timeout');
		expect(revokeLocalToken).not.toHaveBeenCalled();
		expect(listLocalTokens).not.toHaveBeenCalled();
	});

	it('revokes only live old tokens with the same label in all workspaces', async () => {
		listLocalTokens.mockResolvedValue([
			tokenRow({ id: 'lt_old1old1old1old1' }),
			tokenRow({ id: 'lt_old2old2old2old2', workspaceCode: 'w2' }),
			tokenRow({ id: 'lt_newnewnewnewnewn' }),
			tokenRow({ id: 'lt_other0other0oth', label: 'Codex (Connect screen)' }),
			tokenRow({ id: 'lt_revoked0revoked', revokedAt: '2026-01-01' }),
			tokenRow({ id: 'lt_manual0manual00', label: 'My own' }),
		]);
		await installAgent(base);
		expect(listLocalTokens).toHaveBeenCalledWith();
		expect(calls.filter((c) => c.startsWith('revoke:')).sort()).toEqual([
			'revoke:lt_old1old1old1old1',
			'revoke:lt_old2old2old2old2',
		]);
	});

	it('maps the token rate limit to a clear message', async () => {
		issueLocalToken.mockRejectedValueOnce(
			'No more than 10 tokens may be issued per hour',
		);
		const result = await installAgent(base);
		expect(result.status).toBe('failed');
		expect(result.error).toMatch(/10 .*token.*hour/i);
		expect(invoke).not.toHaveBeenCalled();
	});
});

describe('removeAgent', () => {
	it('revokes matching live tokens on success', async () => {
		listLocalTokens.mockResolvedValue([
			tokenRow({ id: 'lt_old1old1old1old1' }),
			tokenRow({ id: 'lt_old2old2old2old2', workspaceCode: 'w2' }),
			tokenRow({ id: 'lt_other0other0oth', label: 'Codex (Connect screen)' }),
		]);
		const result = await removeAgent({ agent: 'claude-code' });
		expect(invoke).toHaveBeenCalledWith('agent_connect_run', {
			agent: 'claude-code',
			action: 'remove',
			tokenId: undefined,
		});
		expect(result.status).toBe('ok');
		expect(calls.filter((c) => c.startsWith('revoke:')).sort()).toEqual([
			'revoke:lt_old1old1old1old1',
			'revoke:lt_old2old2old2old2',
		]);
	});

	it('keeps tokens when the command fails', async () => {
		invoke.mockResolvedValueOnce({ ok: false, timedOut: false, steps: [] });
		const result = await removeAgent({ agent: 'codex' });
		expect(result.status).toBe('failed');
		expect(revokeLocalToken).not.toHaveBeenCalled();
	});
});

describe('configSnippet', () => {
	it('builds the Claude command with --scope user', () => {
		expect(configSnippet('claude-code', '/App/tmgr', 'lt_x')).toBe(
			'claude mcp add --scope user tmgr-local -- "/App/tmgr" mcp --token-id lt_x',
		);
	});

	it('builds Codex TOML and escapes quotes and backslashes', () => {
		expect(configSnippet('codex', '/App/tmgr', 'lt_x')).toBe(
			[
				'[mcp_servers.tmgr-local]',
				'command = "/App/tmgr"',
				'args = ["mcp", "--token-id", "lt_x"]',
			].join('\n'),
		);
		const nasty = '/A "b"/c\\d/tmgr';
		const toml = configSnippet('codex', nasty, 'lt_x');
		expect(toml).toContain('command = "/A \\"b\\"/c\\\\d/tmgr"');
	});

	it('builds Cursor JSON that round-trips awkward paths', () => {
		const path = '/Apps/My "TMGR" app/tmgr';
		const parsed = JSON.parse(configSnippet('cursor', path, 'lt_x'));
		expect(parsed).toEqual({
			mcpServers: {
				'tmgr-local': { command: path, args: ['mcp', '--token-id', 'lt_x'] },
			},
		});
	});
});

describe('copyConfigToken', () => {
	const cursorRow = (over: Record<string, unknown> = {}) =>
		tokenRow({ label: 'Cursor (Connect screen)', ...over });
	beforeEach(() => {
		getLocalAccessStatus.mockResolvedValue({ bridgeCommand: '/App/tmgr' });
	});

	it('reuses a live token with the same label and workspace', async () => {
		listLocalTokens.mockResolvedValue([
			cursorRow({ id: 'lt_gone0gone0gone0', revokedAt: '2026-01-01' }),
			cursorRow({ id: 'lt_otherws0otherws', workspaceCode: 'w2' }),
			cursorRow({ id: 'lt_live0live0live0' }),
		]);
		const result = await copyConfigToken({ ...base, agent: 'cursor' });
		expect(issueLocalToken).not.toHaveBeenCalled();
		expect(result.tokenId).toBe('lt_live0live0live0');
		expect(result.text).toContain('lt_live0live0live0');
	});

	it('does not reuse an expired token', async () => {
		listLocalTokens.mockResolvedValue([
			cursorRow({ expiresAt: new Date(Date.now() - 1000).toISOString() }),
		]);
		const result = await copyConfigToken({ ...base, agent: 'cursor' });
		expect(issueLocalToken).toHaveBeenCalledWith(
			expect.objectContaining({ label: 'Cursor (Connect screen)' }),
		);
		expect(result.tokenId).toBe('lt_newnewnewnewnewn');
	});

	it('issues through the persona and module path when nothing is reusable', async () => {
		const result = await copyConfigToken({ ...base, agent: 'codex' });
		expect(calls).toEqual(['writeFeatures', 'ensurePersona', 'issue']);
		expect(result.modulesChanged).toBe(true);
		expect(result.text).toContain('[mcp_servers.tmgr-local]');
	});

	it('refuses the demo workspace', async () => {
		await expect(
			copyConfigToken({
				...base,
				workspace: { ...workspace, demo: true },
			}),
		).rejects.toThrow(/own workspace/i);
	});
});
