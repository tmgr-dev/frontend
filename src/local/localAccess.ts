import axios from 'axios';
import { createLocalApi } from './api';
import { dispatchLocal } from './dispatch';
import { respond } from './install';
import { handleMcpRequest } from './mcp';
import { checkPersonaIdentity } from './personaGate';
import { readPersonaCache } from './personaCache';
import { normalizePath } from './router';
import { listLocalWorkspaces, localContext } from './runtime';
import { LocalHttpError, type LocalActor, type LocalContext, type LocalUser, type LocalWorkspace } from './types';
import { domainEvents, installDomainEvents } from '@/utils/domainEvents';

export interface LocalAccessRequestPayload {
	id: number;
	workspaceCode: string;
	workspaceId: number;
	personaUuid: string;
	personaName: string;
	tokenId: string;
	method: string;
	path: string;
	body: string | null;
}

export interface LocalAccessReply {
	status: number;
	body: string;
}

export interface LocalAccessTokenInfo {
	prefix: string;
	expiresAt: string;
}

export interface LocalAccessDeps {
	resolveWorkspace: (code: string) => Promise<LocalWorkspace | null>;
	currentUser: () => LocalUser | null;
	contextFor: (workspace: LocalWorkspace, user: LocalUser, actor: LocalActor) => Promise<LocalContext>;
	tokenInfo: (tokenId: string) => Promise<LocalAccessTokenInfo | null>;
	promptVersion: (personaUuid: string) => Promise<number | null>;
}

const errorReply = (status: number, message: string, code: string): LocalAccessReply => ({
	status,
	body: JSON.stringify({ message, code }),
});

const defaultCodeFor = (status: number): string => {
	if (status === 404) return 'NOT_FOUND';
	if (status === 409) return 'CONFLICT';
	if (status === 422) return 'UNPROCESSABLE';
	return 'BAD_REQUEST';
};

const localApi = createLocalApi();

/**
 * One axios instance per request: dispatches into the local router as the persona, and emits the
 * same domain events a UI or plugin write would, with `actor: persona:<uuid>`. The actor never
 * comes from a header, only from this closure's own argument.
 */
const localPersonaClient = (
	ctxFactory: () => Promise<LocalContext>,
	personaUuid: string,
	workspaceId: () => number | null,
) => {
	const instance = axios.create({
		adapter: async (config) => {
			const ctx = await ctxFactory();
			const result = await dispatchLocal(
				localApi,
				ctx,
				config.method ?? 'get',
				config.url ?? '',
				config.data,
				config.params,
			);
			if (!result) {
				return respond(config, 501, {
					message: `Not available in local workspaces: ${(config.method ?? 'get').toUpperCase()} ${config.url}`,
					code: 'NOT_AVAILABLE_LOCALLY',
				});
			}
			return respond(config, result.status, result.data);
		},
	});
	instance.interceptors.request.use((config) => {
		(config as any).localAccessActor = personaUuid;
		return config;
	});
	installDomainEvents(instance, domainEvents, workspaceId);
	return instance;
};

const queryOf = (path: string) => new URLSearchParams(path.includes('?') ? path.slice(path.indexOf('?') + 1) : '');

const parseBody = (raw: string | null): any => {
	if (!raw) return undefined;
	try {
		return JSON.parse(raw);
	} catch {
		return undefined;
	}
};

const whoamiBody = async (
	ctx: LocalContext,
	tokenId: string,
	tokenInfo: LocalAccessTokenInfo | null,
	promptVersion: number | null,
) => {
	const [persona] = await ctx.db.select<{
		name: string;
		description: string | null;
		owner_user_id: number;
		owner_name: string;
	}>(`SELECT name, description, owner_user_id, owner_name FROM personas WHERE uuid = ?`, [ctx.actor!.id]);
	const [grant] = await ctx.db.select<{ permissions: string }>(
		`SELECT permissions FROM workspace_personas WHERE persona_uuid = ?`,
		[ctx.actor!.id],
	);
	const permissions: string[] = grant?.permissions ? JSON.parse(grant.permissions) : [];
	return {
		user_id: ctx.user.id,
		persona: {
			id: ctx.actor!.id,
			name: persona?.name ?? ctx.actor!.name,
			description: persona?.description ?? null,
			owner: { id: persona?.owner_user_id ?? ctx.user.id, name: persona?.owner_name ?? ctx.user.name },
			prompt_version: promptVersion,
			workspace: { id: ctx.workspace.id, code: ctx.workspace.code, permissions },
			skills: [] as unknown[],
		},
		token: { id: tokenId, prefix: tokenInfo?.prefix ?? null, expires_at: tokenInfo?.expiresAt ?? null },
	};
};

/**
 * Answers one bridged request from the socket. Never throws: every failure, including a bug in a
 * handler, becomes a `{message, code}` body so the socket never leaks a stack trace.
 */
export const handleLocalAccessRequest = async (
	payload: LocalAccessRequestPayload,
	deps: LocalAccessDeps,
): Promise<LocalAccessReply> => {
	try {
		const workspace = await deps.resolveWorkspace(payload.workspaceCode);
		if (!workspace) return errorReply(409, 'The local workspace for this token is gone', 'WORKSPACE_GONE');
		if (workspace.id >= 0) return errorReply(403, 'This workspace is not local', 'WORKSPACE_NOT_LOCAL');

		const user = deps.currentUser();
		if (!user) return errorReply(503, 'The app is not ready yet', 'APP_NOT_READY');

		const body = parseBody(payload.body);
		const query = queryOf(payload.path);
		const bodyWorkspaceId =
			body && typeof body === 'object' && 'workspace_id' in body ? Number((body as any).workspace_id) : undefined;
		const queryWorkspaceId = query.has('workspace_id') ? Number(query.get('workspace_id')) : undefined;
		const requestedWorkspaceId = bodyWorkspaceId ?? queryWorkspaceId;
		if (requestedWorkspaceId !== undefined && requestedWorkspaceId !== workspace.id) {
			return errorReply(403, "workspace_id does not match the token's workspace", 'WORKSPACE_MISMATCH');
		}

		const buildCtx = async () => {
			const ctx = await deps.contextFor(workspace, user, {
				kind: 'persona',
				id: payload.personaUuid,
				name: payload.personaName,
			});
			const [personaRow] = await ctx.db.select<{ name: string }>(`SELECT name FROM personas WHERE uuid = ?`, [
				payload.personaUuid,
			]);
			return personaRow?.name ? { ...ctx, actor: { ...ctx.actor!, name: personaRow.name } } : ctx;
		};

		const method = payload.method.toUpperCase();
		const pathOnly = payload.path.split('?')[0];
		const normalized = normalizePath(pathOnly);

		if (normalized === 'local/whoami' && method === 'GET') {
			const ctx = await buildCtx();
			try {
				await checkPersonaIdentity(ctx);
			} catch (error) {
				if (error instanceof LocalHttpError) return errorReply(error.status, error.message, error.code ?? 'BAD_REQUEST');
				throw error;
			}
			const [tokenInfo, promptVersion] = await Promise.all([
				deps.tokenInfo(payload.tokenId),
				deps.promptVersion(payload.personaUuid),
			]);
			return { status: 200, body: JSON.stringify(await whoamiBody(ctx, payload.tokenId, tokenInfo, promptVersion)) };
		}
		if (normalized.startsWith('local/')) {
			return errorReply(404, 'Not found', 'NOT_FOUND');
		}
		if (normalized === 'mcp') {
			const ctx = await buildCtx();
			const result = await handleMcpRequest(localApi, ctx, payload.body ?? '', {
				personaPrompt: (uuid) => readPersonaCache(uuid),
			});
			return { status: result.status, body: result.body };
		}

		const client = localPersonaClient(buildCtx, payload.personaUuid, () => workspace.id);
		try {
			const response = await client.request({ method, url: payload.path, data: body });
			// A Blob (file content) has nothing to gain from a JSON.stringify: {} would look like a
			// valid, empty response instead of the unsupported one it is.
			if (typeof Blob !== 'undefined' && response.data instanceof Blob) {
				return errorReply(501, 'Binary responses are not available over the local socket', 'NOT_AVAILABLE_LOCALLY');
			}
			return { status: response.status, body: JSON.stringify(response.data) };
		} catch (error) {
			if (axios.isAxiosError(error) && error.response) {
				const data = error.response.data as { message?: string; code?: string } | undefined;
				const code = data?.code ?? defaultCodeFor(error.response.status);
				return { status: error.response.status, body: JSON.stringify({ message: data?.message ?? 'Error', code }) };
			}
			throw error;
		}
	} catch (error) {
		console.error('[local-access] request failed', error);
		return { status: 500, body: JSON.stringify({ message: 'Internal error', code: 'INTERNAL' }) };
	}
};

/** Wires the real bridge for the main window: called from `main.ts` only when it is the main window. */
export const installLocalAccess = async (store: any): Promise<void> => {
	const { invoke } = await import('@tauri-apps/api/core');
	const { listen } = await import('@tauri-apps/api/event');
	const { readPersonaCache } = await import('./personaCache');

	const currentUser = (): LocalUser | null => {
		const user = store.state.user;
		return user ? { id: user.id, name: user.name, email: user.email } : null;
	};

	const notifyReady = () => {
		invoke('local_access_ready', { userId: currentUser()?.id ?? null }).catch((error: unknown) => {
			console.error('[local-access] failed to report readiness', error);
		});
	};
	const deps: LocalAccessDeps = {
		resolveWorkspace: async (code) => (await listLocalWorkspaces(true)).find((w) => w.code === code) ?? null,
		currentUser,
		contextFor: localContext,
		tokenInfo: async (tokenId) => {
			const tokens = await invoke<{ id: string; prefix: string; expiresAt: string }[]>('local_token_list', {});
			const found = tokens.find((t) => t.id === tokenId);
			return found ? { prefix: found.prefix, expiresAt: found.expiresAt } : null;
		},
		promptVersion: async (uuid) => {
			const cached = await readPersonaCache(uuid);
			return cached?.prompt_version ?? null;
		},
	};

	// `ready` means both a user is logged in AND this handler exists to answer requests, so the
	// listener goes up first: readiness must never be announced before there is anyone to ask.
	await listen<LocalAccessRequestPayload>('local-access://request', (event) => {
		void handleLocalAccessRequest(event.payload, deps).then((reply) =>
			invoke('local_access_reply', { id: event.payload.id, status: reply.status, body: reply.body }).catch(
				(error: unknown) => console.error('[local-access] failed to send reply', error),
			),
		);
	});

	store.watch((state: any) => state.user, notifyReady);
	notifyReady();
};
