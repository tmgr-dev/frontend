const detectAgents = jest.fn();
const installAgent = jest.fn();
const removeAgent = jest.fn();
const copyConfigToken = jest.fn();
jest.mock('@/local/connectAgent', () => ({
	...jest.requireActual('@/local/connectAgent'),
	detectAgents: (...a: any[]) => detectAgents(...a),
	installAgent: (...a: any[]) => installAgent(...a),
	removeAgent: (...a: any[]) => removeAgent(...a),
	copyConfigToken: (...a: any[]) => copyConfigToken(...a),
}));

import {
	CLAUDE_FAILURE_NOTE,
	INSTALL_NOTES,
	SAFE_MODE_TEXT,
	TIMEOUT_TEXT,
	TRANSLOCATED_TEXT,
	useConnectLocalAgent,
} from '@/composable/useConnectLocalAgent';

const workspace = { id: 1, name: 'W', code: 'w1' } as any;
const user = { id: -1, name: 'Me', email: '' };

const detected = (over: Record<string, unknown> = {}) => ({
	platformSupported: true,
	safeMode: false,
	bridge: { ok: true, reason: null },
	agents: [
		{
			id: 'claude-code',
			found: true,
			cli: '~/bin/claude',
			install: [
				['~/bin/claude', 'mcp', 'remove', 'tmgr-local'],
				[
					'~/bin/claude',
					'mcp',
					'add',
					'--',
					'/Applications/My App/tmgr',
					'--token-id',
					'<new token id>',
				],
			],
			remove: [['~/bin/claude', 'mcp', 'remove', 'tmgr-local']],
		},
		{ id: 'codex', found: false, cli: null, install: [], remove: [] },
	],
	...over,
});

const make = async (over: Record<string, unknown> = {}) => {
	detectAgents.mockResolvedValue(detected(over));
	const refresh = jest.fn();
	const writeClipboard = jest.fn(async () => undefined);
	const state = useConnectLocalAgent({
		workspace: () => workspace,
		user: () => user,
		onModulesChanged: refresh,
		writeClipboard,
	});
	await state.load();
	return { state, refresh, writeClipboard };
};

beforeEach(() => jest.clearAllMocks());

describe('rows', () => {
	it('lists three agents with found state and checkbox rules', async () => {
		const { state } = await make();
		expect(
			state.rows.value.map((r) => [
				r.info.id,
				r.found,
				r.selectable,
				r.copyOnly,
			]),
		).toEqual([
			['claude-code', true, true, false],
			['codex', false, false, true],
			['cursor', false, false, true],
		]);
	});

	it('is copy-only everywhere when the platform is unsupported', async () => {
		const { state } = await make({ platformSupported: false });
		expect(state.rows.value.every((r) => r.copyOnly && !r.selectable)).toBe(
			true,
		);
	});

	it('is copy-only with no checkbox when install is blocked', async () => {
		for (const over of [
			{ safeMode: true },
			{ bridge: { ok: false, reason: 'translocated' } },
			{ bridge: { ok: false, reason: 'invalid' } },
		]) {
			const { state } = await make(over);
			const claude = state.rows.value.find((r) => r.info.id === 'claude-code')!;
			expect(claude.found).toBe(true);
			expect(claude.selectable).toBe(false);
			expect(claude.copyOnly).toBe(true);
			state.toggle('claude-code', true);
			expect(state.canPreview.value).toBe(false);
		}
	});

	it('survives a failing detect', async () => {
		detectAgents.mockRejectedValue('nope');
		const state = useConnectLocalAgent({
			workspace: () => workspace,
			user: () => user,
		});
		await state.load();
		expect(state.loadError.value).toBe('nope');
		expect(state.rows.value).toHaveLength(3);
	});
});

describe('preview and install gating', () => {
	it('keeps Install disabled until Preview', async () => {
		const { state } = await make();
		state.toggle('claude-code', true);
		expect(state.canPreview.value).toBe(true);
		expect(state.canInstall.value).toBe(false);
		state.preview();
		expect(state.canInstall.value).toBe(true);
	});

	it('shows the argv steps of selected agents and the three notes', async () => {
		const { state } = await make();
		state.toggle('claude-code', true);
		state.preview();
		expect(state.previewLines.value).toEqual([
			{
				agent: 'claude-code',
				title: 'Claude Code',
				commands: [
					'~/bin/claude mcp remove tmgr-local',
					"~/bin/claude mcp add -- '/Applications/My App/tmgr' --token-id <new token id>",
				],
			},
		]);
		expect(INSTALL_NOTES).toEqual([
			'Install also turns on local agent access and the Personas and Agent work modules in this workspace.',
			'Your token stays in the macOS Keychain. It is not part of the command.',
			'Each agent connects to one workspace. Installing here replaces an earlier TMGR connection.',
		]);
	});

	it('resets Preview when the selection changes', async () => {
		const { state } = await make();
		state.toggle('claude-code', true);
		state.preview();
		state.toggle('claude-code', false);
		expect(state.canInstall.value).toBe(false);
	});

	it('disables Install for a translocated bridge', async () => {
		const { state } = await make({
			bridge: { ok: false, reason: 'translocated' },
		});
		state.toggle('claude-code', true);
		state.preview();
		expect(state.canInstall.value).toBe(false);
		expect(state.installBlock.value).toBe(TRANSLOCATED_TEXT);
		expect(TRANSLOCATED_TEXT).toBe(
			'Move TMGR to Applications, open it from there, then install.',
		);
	});

	it('disables Install in safe mode', async () => {
		const { state } = await make({ safeMode: true });
		state.toggle('claude-code', true);
		state.preview();
		expect(state.canInstall.value).toBe(false);
		expect(state.installBlock.value).toBe(SAFE_MODE_TEXT);
	});

	it('disables Install without platform support', async () => {
		const { state } = await make({ platformSupported: false });
		expect(state.installBlock.value).toMatch(/macOS/);
		expect(state.canInstall.value).toBe(false);
	});
});

describe('install', () => {
	const ready = async (over: Record<string, unknown> = {}) => {
		const made = await make(over);
		made.state.toggle('claude-code', true);
		made.state.preview();
		return made;
	};

	it('installs, shows the restart hint and refreshes modules', async () => {
		installAgent.mockResolvedValue({
			status: 'ok',
			modulesChanged: true,
			outcome: {
				ok: true,
				timedOut: false,
				steps: [
					{
						display: ['claude', 'mcp', 'add', 'a b'],
						exitCode: 0,
						output: 'added',
						tolerated: false,
					},
				],
			},
		});
		const { state, refresh } = await ready();
		await state.install();
		expect(installAgent).toHaveBeenCalledWith({
			agent: 'claude-code',
			workspace,
			user,
		});
		const result = state.results['claude-code'];
		expect(result.status).toBe('ok');
		expect(result.message).toBe(
			'Restart Claude Code so it picks up TMGR. Then ask it to list your TMGR tasks.',
		);
		expect(result.steps[0]).toEqual({
			command: "claude mcp add 'a b'",
			output: 'added',
		});
		expect(refresh).toHaveBeenCalledTimes(1);
	});

	it('ignores a second click while running', async () => {
		let release: (v: unknown) => void = () => undefined;
		installAgent.mockReturnValue(new Promise((r) => (release = r)));
		const { state } = await ready();
		const first = state.install();
		await state.install();
		expect(installAgent).toHaveBeenCalledTimes(1);
		expect(state.running.value).toBe(true);
		release({ status: 'ok', modulesChanged: false, outcome: null });
		await first;
		expect(state.running.value).toBe(false);
	});

	it('shows the error, output and the Claude note on failure', async () => {
		installAgent.mockResolvedValue({
			status: 'failed',
			modulesChanged: false,
			outcome: {
				ok: false,
				timedOut: false,
				steps: [
					{
						display: ['claude'],
						exitCode: 1,
						output: 'boom',
						tolerated: false,
					},
				],
			},
		});
		const { state } = await ready();
		await state.install();
		const result = state.results['claude-code'];
		expect(result.status).toBe('failed');
		expect(result.steps[0].output).toBe('boom');
		expect(result.note).toBe(CLAUDE_FAILURE_NOTE);
		expect(CLAUDE_FAILURE_NOTE).toBe(
			'The earlier TMGR entry in Claude Code was already removed.',
		);
	});

	it('omits the Claude note when no command ran', async () => {
		for (const outcome of [null, { ok: false, timedOut: false, steps: [] }]) {
			installAgent.mockResolvedValue({
				status: 'failed',
				modulesChanged: false,
				outcome,
				error: 'The agent CLI is not trusted',
			});
			const { state } = await ready();
			await state.install();
			const result = state.results['claude-code'];
			expect(result.status).toBe('failed');
			expect(result.note).toBeUndefined();
		}
	});

	it('explains a timeout', async () => {
		installAgent.mockResolvedValue({
			status: 'timeout',
			modulesChanged: false,
			outcome: { ok: false, timedOut: true, steps: [] },
		});
		const { state } = await ready();
		await state.install();
		expect(state.results['claude-code'].message).toBe(TIMEOUT_TEXT);
		expect(TIMEOUT_TEXT).toBe(
			'The command did not finish. Press Install again, or Remove.',
		);
	});

	it('shows a plain error when nothing ran', async () => {
		installAgent.mockResolvedValue({
			status: 'failed',
			modulesChanged: false,
			outcome: null,
			error: 'CLI not found',
		});
		const { state } = await ready();
		await state.install();
		expect(state.results['claude-code'].message).toBe('CLI not found');
	});
});

describe('remove and copy config', () => {
	it('removes an agent and reports it', async () => {
		removeAgent.mockResolvedValue({
			status: 'ok',
			modulesChanged: false,
			outcome: { ok: true, timedOut: false, steps: [] },
		});
		const { state } = await make();
		await state.remove('claude-code');
		expect(removeAgent).toHaveBeenCalledWith({ agent: 'claude-code' });
		expect(state.results['claude-code'].message).toBe(
			'Removed TMGR from Claude Code.',
		);
	});

	it('copies the config text and refreshes modules when they changed', async () => {
		copyConfigToken.mockResolvedValue({
			text: 'CFG',
			tokenId: 'lt_x',
			reused: false,
			modulesChanged: true,
		});
		const { state, writeClipboard, refresh } = await make();
		await state.copyConfig('cursor');
		expect(copyConfigToken).toHaveBeenCalledWith({
			agent: 'cursor',
			workspace,
			user,
		});
		expect(writeClipboard).toHaveBeenCalledWith('CFG');
		expect(state.copied.value).toBe('cursor');
		expect(refresh).toHaveBeenCalled();
	});

	it('reports a copy failure', async () => {
		copyConfigToken.mockRejectedValue(new Error('rate'));
		const { state } = await make();
		await state.copyConfig('cursor');
		expect(state.copyError.value).toBe('rate');
	});
});
