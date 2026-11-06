import { DEFAULT_TOKEN_EXPIRY_DAYS } from '@/utils/personas';
import {
	getLocalAccessStatus,
	issueLocalToken,
	listLocalTokens,
	revokeLocalToken,
	type TokenInfo,
} from './localTokens';
import { localModulesPayload, writeFeatures } from './modulesApi';
import {
	enableLocalPersona,
	ensureDefaultLocalPersona,
	listLocalPersonas,
} from './personas';
import { localContext } from './runtime';
import type { LocalUser, LocalWorkspace } from './types';

export type ConnectAgentId = 'claude-code' | 'codex' | 'cursor';
export type RunnableAgentId = 'claude-code' | 'codex';
export type ConnectAction = 'install' | 'remove';

export interface ConnectAgentInfo {
	id: ConnectAgentId;
	title: string;
	canInstall: boolean;
	tokenLabel: string;
}

export const CONNECT_AGENTS: ConnectAgentInfo[] = [
	{
		id: 'claude-code',
		title: 'Claude Code',
		canInstall: true,
		tokenLabel: 'Claude Code (Connect screen)',
	},
	{
		id: 'codex',
		title: 'Codex',
		canInstall: true,
		tokenLabel: 'Codex (Connect screen)',
	},
	{
		id: 'cursor',
		title: 'Cursor',
		canInstall: false,
		tokenLabel: 'Cursor (Connect screen)',
	},
];

export const agentInfo = (id: ConnectAgentId): ConnectAgentInfo =>
	CONNECT_AGENTS.find((a) => a.id === id)!;

export interface DetectedAgent {
	id: RunnableAgentId;
	found: boolean;
	cli: string | null;
	install: string[][];
	remove: string[][];
}

export interface DetectOut {
	platformSupported: boolean;
	safeMode: boolean;
	bridge: {
		ok: boolean;
		reason: null | 'translocated' | 'invalid' | 'unsupported';
	};
	agents: DetectedAgent[];
}

export interface StepOutcome {
	display: string[];
	exitCode: number | null;
	output: string;
	tolerated: boolean;
}

export interface RunOutcome {
	ok: boolean;
	timedOut: boolean;
	steps: StepOutcome[];
}

const invoke = async <T>(
	command: string,
	args?: Record<string, unknown>,
): Promise<T> => {
	const core = await import('@tauri-apps/api/core');
	return core.invoke<T>(command, args);
};

export const detectAgents = (): Promise<DetectOut> => invoke('agents_detect');

export const runAgentConnect = (
	agent: RunnableAgentId,
	action: ConnectAction,
	tokenId?: string,
): Promise<RunOutcome> =>
	invoke('agent_connect_run', { request: { agent, action, tokenId } });

export const DEMO_REFUSAL =
	'Create your own workspace first, then connect your agent.';
const RATE_LIMIT_MESSAGE =
	'You have already created 10 agent tokens in the last hour. Wait a little and try again.';

export const errorText = (error: unknown): string => {
	const text =
		typeof error === 'string'
			? error
			: error instanceof Error
				? error.message
				: String(error);
	return /10 tokens|per hour/i.test(text) ? RATE_LIMIT_MESSAGE : text;
};

const MODULE_KEYS = ['personas', 'agent_work'];

const prepareWorkspace = async (workspace: LocalWorkspace, user: LocalUser) => {
	const ctx = await localContext(workspace, user);
	const payload = await localModulesPayload(ctx);
	const off = MODULE_KEYS.filter(
		(key) =>
			!(payload.modules as { key: string; enabled: boolean }[]).find(
				(m) => m.key === key,
			)?.enabled,
	);
	if (off.length)
		await writeFeatures(ctx, Object.fromEntries(off.map((k) => [k, true])));
	const { uuid } = await ensureDefaultLocalPersona(ctx);
	const row = (await listLocalPersonas(ctx)).find((p) => p.uuid === uuid);
	if (row?.disabled_at)
		await enableLocalPersona(ctx, uuid, row.permissions ?? []);
	return { ctx, uuid, name: row?.name ?? '', modulesChanged: off.length > 0 };
};

const issueFor = async (
	agent: ConnectAgentInfo,
	workspace: LocalWorkspace,
	user: LocalUser,
) => {
	const prepared = await prepareWorkspace(workspace, user);
	const token = await issueLocalToken({
		personaUuid: prepared.uuid,
		personaName: prepared.name,
		workspaceCode: workspace.code,
		label: agent.tokenLabel,
		expiresInDays: DEFAULT_TOKEN_EXPIRY_DAYS,
	});
	return { token, modulesChanged: prepared.modulesChanged };
};

const isLive = (token: TokenInfo) =>
	!token.revokedAt &&
	!token.pluginId &&
	new Date(token.expiresAt).getTime() > Date.now();

const revokeLabelled = async (label: string, keepId?: string) => {
	const tokens = await listLocalTokens();
	for (const token of tokens) {
		if (token.label !== label || token.id === keepId || !isLive(token))
			continue;
		try {
			await revokeLocalToken(token.id);
		} catch (error) {
			console.error('Failed to revoke an old agent token', error);
		}
	}
};

export interface ConnectResult {
	status: 'ok' | 'failed' | 'timeout';
	outcome: RunOutcome | null;
	error?: string;
	modulesChanged: boolean;
}

export const installAgent = async (args: {
	agent: RunnableAgentId;
	workspace: LocalWorkspace;
	user: LocalUser;
}): Promise<ConnectResult> => {
	const { agent, workspace, user } = args;
	if (workspace.demo)
		return {
			status: 'failed',
			outcome: null,
			error: DEMO_REFUSAL,
			modulesChanged: false,
		};
	const info = agentInfo(agent);
	let modulesChanged = false;
	let tokenId: string | null = null;
	try {
		const issued = await issueFor(info, workspace, user);
		modulesChanged = issued.modulesChanged;
		tokenId = issued.token.id;
		const outcome = await runAgentConnect(agent, 'install', tokenId);
		if (outcome.timedOut) return { status: 'timeout', outcome, modulesChanged };
		if (!outcome.ok) {
			await revokeLocalToken(tokenId);
			return { status: 'failed', outcome, modulesChanged };
		}
		await revokeLabelled(info.tokenLabel, tokenId);
		return { status: 'ok', outcome, modulesChanged };
	} catch (error) {
		if (tokenId) await revokeLocalToken(tokenId).catch(() => undefined);
		return {
			status: 'failed',
			outcome: null,
			error: errorText(error),
			modulesChanged,
		};
	}
};

export const removeAgent = async (args: {
	agent: RunnableAgentId;
}): Promise<ConnectResult> => {
	try {
		const outcome = await runAgentConnect(args.agent, 'remove');
		if (outcome.timedOut)
			return { status: 'timeout', outcome, modulesChanged: false };
		if (!outcome.ok)
			return { status: 'failed', outcome, modulesChanged: false };
		await revokeLabelled(agentInfo(args.agent).tokenLabel);
		return { status: 'ok', outcome, modulesChanged: false };
	} catch (error) {
		return {
			status: 'failed',
			outcome: null,
			error: errorText(error),
			modulesChanged: false,
		};
	}
};

const escapeToml = (value: string) =>
	value.replace(/\\/g, '\\\\').replace(/"/g, '\\"');

const escapeShellDouble = (value: string) =>
	value.replace(/([\\"$`])/g, '\\$1');

export const configSnippet = (
	agent: ConnectAgentId,
	bridgeCommand: string,
	tokenId: string,
): string => {
	if (agent === 'codex')
		return [
			'[mcp_servers.tmgr-local]',
			`command = "${escapeToml(bridgeCommand)}"`,
			`args = ["mcp", "--token-id", "${tokenId}"]`,
		].join('\n');
	if (agent === 'cursor')
		return JSON.stringify(
			{
				mcpServers: {
					'tmgr-local': {
						command: bridgeCommand,
						args: ['mcp', '--token-id', tokenId],
					},
				},
			},
			null,
			2,
		);
	return `claude mcp add --scope user tmgr-local -- "${escapeShellDouble(bridgeCommand)}" mcp --token-id ${tokenId}`;
};

export const copyConfigToken = async (args: {
	agent: ConnectAgentId;
	workspace: LocalWorkspace;
	user: LocalUser;
}): Promise<{
	text: string;
	tokenId: string;
	reused: boolean;
	modulesChanged: boolean;
}> => {
	const { agent, workspace, user } = args;
	if (workspace.demo) throw new Error(DEMO_REFUSAL);
	const info = agentInfo(agent);
	const existing = (await listLocalTokens()).find(
		(t) =>
			t.label === info.tokenLabel &&
			t.workspaceCode === workspace.code &&
			isLive(t),
	);
	let tokenId: string;
	let modulesChanged = false;
	if (existing) tokenId = existing.id;
	else {
		try {
			const issued = await issueFor(info, workspace, user);
			tokenId = issued.token.id;
			modulesChanged = issued.modulesChanged;
		} catch (error) {
			throw new Error(errorText(error));
		}
	}
	const status = await getLocalAccessStatus();
	return {
		text: configSnippet(agent, status.bridgeCommand, tokenId),
		tokenId,
		reused: !!existing,
		modulesChanged,
	};
};
