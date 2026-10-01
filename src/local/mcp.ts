import { dispatchLocal } from './dispatch';
import { normalizePath, type LocalRouter } from './router';
import type { LocalContext } from './types';
import { checkPersonaIdentity, type PersonaPermission } from './personaGate';

export interface McpDeps {
	personaPrompt(uuid: string): Promise<{ system_prompt?: string | null; prompt_version?: number | null } | null>;
}

class ToolError extends Error {}

const SUPPORTED_PROTOCOL_VERSIONS = ['2025-06-18', '2025-03-26', '2024-11-05'];
const DEFAULT_PROTOCOL_VERSION = '2025-06-18';

const toCamelKey = (key: string): string => key.replace(/_([a-z0-9])/g, (_, c) => c.toUpperCase());

/** Free-form user content (rich-text JSON, checkpoint lists): keys travel as the user wrote them. */
const OPAQUE_KEYS = new Set(['description_json', 'checkpoints', 'settings', 'properties']);

const camelizeDeep = (value: unknown, opaque = false): unknown => {
	if (opaque) return value;
	if (Array.isArray(value)) return value.map((v) => camelizeDeep(v));
	if (value && typeof value === 'object') {
		return Object.fromEntries(
			Object.entries(value).map(([k, v]) => [toCamelKey(k), camelizeDeep(v, OPAQUE_KEYS.has(k))]),
		);
	}
	return value;
};

const ensureTokenWorkspace = (args: Record<string, any>, ctx: LocalContext) => {
	if (args.workspaceId != null && Number(args.workspaceId) !== ctx.workspace.id) {
		throw new ToolError('workspaceId does not match this local workspace');
	}
};

const requireArg = (args: Record<string, any>, key: string): any => {
	if (args[key] === undefined || args[key] === null || args[key] === '') {
		throw new ToolError(`${key} is required`);
	}
	return args[key];
};

const runRoute = async (
	router: LocalRouter,
	ctx: LocalContext,
	method: string,
	path: string,
	body?: unknown,
): Promise<any> => {
	const res = await dispatchLocal(router, ctx, method, path, body);
	if (!res) throw new ToolError(`No local route for ${method} ${path}`);
	if (res.status >= 400) throw new ToolError(res.data?.message ?? `Request failed with status ${res.status}`);
	return res.data?.data;
};

const qs = (params: Record<string, unknown>): string => {
	const search = new URLSearchParams();
	Object.entries(params).forEach(([key, value]) => {
		if (value !== undefined && value !== null && value !== '') search.set(key, String(value));
	});
	const s = search.toString();
	return s ? `?${s}` : '';
};

const pageOf = async (
	router: LocalRouter,
	ctx: LocalContext,
	path: string,
): Promise<{ items: any[]; page: number; perPage: number; total: number; hasMore: boolean }> => {
	const res = await dispatchLocal(router, ctx, 'GET', path);
	if (!res) throw new ToolError(`No local route for GET ${path}`);
	if (res.status >= 400) throw new ToolError(res.data?.message ?? `Request failed with status ${res.status}`);
	const { data, meta } = res.data;
	return { items: data, page: meta.current_page, perPage: meta.per_page, total: meta.total, hasMore: meta.current_page < meta.last_page };
};

const DEFAULT_PER_PAGE = 50;
const MAX_PER_PAGE = 100;

/** Cloud pagination (`McpPage.of`) for a local route that has none: slice the full array in memory. */
const paginateInMemory = (all: any[], page?: number, perPage?: number) => {
	const p = !page || page < 1 ? 1 : page;
	const size = !perPage || perPage < 1 ? DEFAULT_PER_PAGE : Math.min(perPage, MAX_PER_PAGE);
	const from = Math.min((p - 1) * size, all.length);
	const to = Math.min(from + size, all.length);
	return { items: all.slice(from, to), page: p, perPage: size, total: all.length, hasMore: to < all.length };
};

/** The local `tasks` route defaults `per_page` to 20; the cloud tools default to 50 (cap 100). */
const perPageArg = (args: Record<string, any>): number =>
	!args.perPage || args.perPage < 1 ? DEFAULT_PER_PAGE : Math.min(Number(args.perPage), MAX_PER_PAGE);

/** Empty permissions (never the stored grant) once a persona is unknown, archived or disabled. */
const personaGrantPermissions = async (ctx: LocalContext): Promise<PersonaPermission[]> => {
	const [persona] = await ctx.db.select<{ archived_at: string | null }>(
		`SELECT archived_at FROM personas WHERE uuid = ?`,
		[ctx.actor!.id],
	);
	if (!persona || persona.archived_at) return [];
	const [grant] = await ctx.db.select<{ permissions: string; disabled_at: string | null }>(
		`SELECT permissions, disabled_at FROM workspace_personas WHERE persona_uuid = ?`,
		[ctx.actor!.id],
	);
	if (!grant || grant.disabled_at) return [];
	return JSON.parse(grant.permissions || '[]');
};

const personaRow = (ctx: LocalContext) =>
	ctx.db
		.select<{ name: string; description: string | null; owner_user_id: number; owner_name: string }>(
			`SELECT name, description, owner_user_id, owner_name FROM personas WHERE uuid = ?`,
			[ctx.actor!.id],
		)
		.then((rows) => rows[0] ?? null);

interface ToolDef {
	name: string;
	description: string;
	inputSchema: { type: 'object'; properties: Record<string, unknown>; required?: string[] };
	permission?: PersonaPermission;
	/** `user` tools are for the account owner's own token; a persona never sees or runs them. */
	audience?: 'user';
	raw?: boolean;
	/** `whoami`/`get_persona` mirror the cloud's own snake_case Map literally — never camelized. */
	literal?: boolean;
	handler: (args: Record<string, any>, ctx: LocalContext, router: LocalRouter, deps: McpDeps) => Promise<any>;
}


/** Page failures read like the cloud tool's: the message plus status and error code, a conflict names the current version. */
const pageRoute = async (router: LocalRouter, ctx: LocalContext, method: string, path: string, body?: unknown) => {
	const res = await dispatchLocal(router, ctx, method, path, body);
	if (!res) throw new ToolError(`No local route for ${method} ${path}`);
	if (res.status >= 400) {
		const error = res.data?.error ?? res.data?.code;
		if (res.status === 409 && res.data?.data?.version != null) {
			throw new ToolError(
				`Page version conflict (409 page_conflict): the page is now at version ${res.data.data.version}. Re-read it with pages_get, merge your change into the current body and retry with that version; never overwrite.`,
			);
		}
		throw new ToolError(`${res.data?.message ?? 'Request failed'} (${res.status}${error ? ` ${error}` : ''})`);
	}
	return res.data?.data;
};

const pageItems = (items: any[]) => ({ items, total: items.length });

const WORKSPACE_ARG = "Workspace id; omit for the account's default workspace (a persona always works in its token's workspace)";
const workspaceProp = { type: 'number', description: WORKSPACE_ARG };
const pageBody = (args: Record<string, any>, keys: Record<string, string>) => {
	const body: Record<string, any> = {};
	for (const [arg, field] of Object.entries(keys)) if (args[arg] !== undefined && args[arg] !== null) body[field] = args[arg];
	return body;
};
const pageRef = (args: Record<string, any>) => encodeURIComponent(String(requireArg(args, 'id')));

const PAGES_TOOLS: ToolDef[] = [
	{
		name: 'workspace_context',
		description:
			'Bodies of all context pages of the workspace as one markdown document (each page under a `# title` heading, pinned first). It is the shared memory of the project: how we work, architecture, notes from earlier agent sessions. AGENT RULES: call workspace_context and pages_for_task before working on a task. In a context page you may write ONLY the section "Agent notes" (section id `agent-notes`): use pages_append without a heading, or pages_set_section with sectionId agent-notes.',
		permission: 'pages:read',
		raw: true,
		inputSchema: { type: 'object', properties: { workspaceId: workspaceProp } },
		async handler(args, ctx, router) {
			ensureTokenWorkspace(args, ctx);
			return (await pageRoute(router, ctx, 'GET', 'workspaces/context')).markdown;
		},
	},
	{
		name: 'pages_for_task',
		description:
			'Pages that mention a task (backlinks of the task). AGENT RULES: call this and workspace_context before working on a task, and read the pages it returns with pages_get',
		permission: 'pages:read',
		inputSchema: {
			type: 'object',
			properties: { taskId: { type: 'number', description: 'Task id' }, workspaceId: workspaceProp },
			required: ['taskId'],
		},
		async handler(args, ctx, router) {
			ensureTokenWorkspace(args, ctx);
			const taskId = Number(requireArg(args, 'taskId'));
			if (!Number.isSafeInteger(taskId) || taskId <= 0) throw new ToolError('taskId must be a positive integer');
			return pageItems(await pageRoute(router, ctx, 'GET', `tasks/${taskId}/pages`));
		},
	},
	{
		name: 'pages_search',
		description:
			'Full-text search over page titles and bodies of a workspace. Returns id, slug, title, type and a snippet; never pages of other workspaces or deleted pages',
		permission: 'pages:read',
		inputSchema: {
			type: 'object',
			properties: {
				query: { type: 'string', description: 'Search words' },
				type: { type: 'string', description: 'Page type filter: plain, context, person or meeting' },
				limit: { type: 'number', description: 'Max hits (default 20, max 50)' },
				workspaceId: workspaceProp,
			},
			required: ['query'],
		},
		async handler(args, ctx, router) {
			ensureTokenWorkspace(args, ctx);
			const hits = await pageRoute(
				router,
				ctx,
				'GET',
				`pages/search${qs({ q: requireArg(args, 'query'), type: args.type, limit: args.limit })}`,
			);
			return pageItems(hits);
		},
	},
	{
		name: 'pages_tree',
		description:
			'All pages of a workspace as a flat list with parentId and position (a tree without bodies). Use pages_get for the body',
		permission: 'pages:read',
		inputSchema: { type: 'object', properties: { workspaceId: workspaceProp } },
		async handler(args, ctx, router) {
			ensureTokenWorkspace(args, ctx);
			return pageItems(await pageRoute(router, ctx, 'GET', 'pages/tree'));
		},
	},
	{
		name: 'pages_get',
		description:
			'A page by numeric id or slug: body (markdown), properties, version, sections with their owners, backlinks. Remember its `version`: pages_update needs it',
		permission: 'pages:read',
		inputSchema: {
			type: 'object',
			properties: { id: { type: 'string', description: 'Page id or slug' }, workspaceId: workspaceProp },
			required: ['id'],
		},
		async handler(args, ctx, router) {
			ensureTokenWorkspace(args, ctx);
			return pageRoute(router, ctx, 'GET', `pages/${pageRef(args)}`);
		},
	},
	{
		name: 'pages_create',
		description:
			"Create a page. type is plain (default), context (people only), person or meeting. Body defaults to the type's template. Returns the page with its id and slug",
		permission: 'pages:write',
		inputSchema: {
			type: 'object',
			properties: {
				title: { type: 'string', description: 'Page title' },
				type: { type: 'string', description: 'plain (default), context, person or meeting' },
				parentId: { type: 'number', description: 'Parent page id' },
				body: { type: 'string', description: 'Markdown body' },
				properties: { type: 'object', description: 'Properties object' },
				workspaceId: workspaceProp,
			},
			required: ['title'],
		},
		async handler(args, ctx, router) {
			ensureTokenWorkspace(args, ctx);
			requireArg(args, 'title');
			const body = pageBody(args, { title: 'title', type: 'type', parentId: 'parent_id', body: 'body', properties: 'properties' });
			return pageRoute(router, ctx, 'POST', 'pages', body);
		},
	},
	{
		name: 'pages_update',
		description:
			"Replace the title, body or properties of a page. AGENT RULES: pass the `version` from the LATEST pages_get of this page. On a version conflict the tool fails and states the current version: re-read the page with pages_get, merge your change into the new body and retry; never overwrite someone else's edit. summary is one line about what changed. For adding text prefer pages_append: it needs no version. In context pages agents may change only the section 'Agent notes'",
		permission: 'pages:write',
		inputSchema: {
			type: 'object',
			properties: {
				id: { type: 'string', description: 'Page id or slug' },
				version: { type: 'number', description: 'Version from the latest pages_get' },
				body: { type: 'string', description: 'Whole new markdown body' },
				title: { type: 'string', description: 'New title' },
				properties: { type: 'object', description: 'New properties object' },
				summary: { type: 'string', description: 'One line: what changed' },
				workspaceId: workspaceProp,
			},
			required: ['id', 'version'],
		},
		async handler(args, ctx, router) {
			ensureTokenWorkspace(args, ctx);
			requireArg(args, 'version');
			const body = pageBody(args, { version: 'version', body: 'body', title: 'title', properties: 'properties', summary: 'summary' });
			return pageRoute(router, ctx, 'PATCH', `pages/${pageRef(args)}`, body);
		},
	},
	{
		name: 'pages_append',
		description:
			"Append markdown to the end of a page, or under a heading, without a version (safe against concurrent edits). AGENT RULES: a session summary longer than a few lines goes here into the task's page, a meeting page or a context page instead of a new comment. On a context page without a heading it lands in the section 'Agent notes' (`agent-notes`), the only place agents may write there. summary is one line about what changed",
		permission: 'pages:write',
		inputSchema: {
			type: 'object',
			properties: {
				id: { type: 'string', description: 'Page id or slug' },
				markdown: { type: 'string', description: 'Markdown to append' },
				heading: { type: 'string', description: 'Append under this existing heading (text without #)' },
				createHeading: { type: 'boolean', description: 'Create the heading at the end when it does not exist' },
				summary: { type: 'string', description: 'One line: what changed' },
				workspaceId: workspaceProp,
			},
			required: ['id', 'markdown'],
		},
		async handler(args, ctx, router) {
			ensureTokenWorkspace(args, ctx);
			requireArg(args, 'markdown');
			const body = pageBody(args, { markdown: 'markdown', heading: 'heading', createHeading: 'create_heading', summary: 'summary' });
			return pageRoute(router, ctx, 'POST', `pages/${pageRef(args)}/append`, body);
		},
	},
	{
		name: 'pages_set_section',
		description:
			"Replace the content of one managed section of a page (sections are the pages' `sections` list from pages_get). Fails with 403 when the section is not yours: on context pages agents may write only sectionId `agent-notes` ('Agent notes'). summary is one line about what changed",
		permission: 'pages:write',
		inputSchema: {
			type: 'object',
			properties: {
				id: { type: 'string', description: 'Page id or slug' },
				sectionId: { type: 'string', description: 'Section id, e.g. agent-notes' },
				markdown: { type: 'string', description: 'New markdown content of the section' },
				summary: { type: 'string', description: 'One line: what changed' },
				workspaceId: workspaceProp,
			},
			required: ['id', 'sectionId', 'markdown'],
		},
		async handler(args, ctx, router) {
			ensureTokenWorkspace(args, ctx);
			const sectionId = encodeURIComponent(String(requireArg(args, 'sectionId')));
			if (typeof args.markdown !== 'string') throw new ToolError('markdown is required');
			const body = pageBody(args, { markdown: 'markdown', summary: 'summary' });
			return pageRoute(router, ctx, 'PUT', `pages/${pageRef(args)}/sections/${sectionId}`, body);
		},
	},
];

const TOOLS: ToolDef[] = [
	{
		name: 'whoami',
		description: "Returns the authenticated user id and the acting persona's identity, owner, workspace and permissions",
		inputSchema: { type: 'object', properties: {} },
		literal: true,
		async handler(_args, ctx, _router, deps) {
			const persona = await personaRow(ctx);
			const permissions = await personaGrantPermissions(ctx);
			const prompt = await deps.personaPrompt(ctx.actor!.id);
			return {
				user_id: ctx.user.id,
				persona: {
					id: ctx.actor!.id,
					name: persona?.name ?? ctx.actor!.name,
					description: persona?.description ?? null,
					owner: { id: persona?.owner_user_id ?? ctx.user.id, name: persona?.owner_name ?? ctx.user.name },
					prompt_version: prompt?.prompt_version ?? null,
					workspace: { id: ctx.workspace.id, code: ctx.workspace.code, permissions },
					skills: [],
				},
			};
		},
	},
	{
		name: 'get_persona',
		description: "Returns the acting persona's system prompt, its version, and the skills index",
		inputSchema: { type: 'object', properties: {} },
		literal: true,
		async handler(_args, ctx, _router, deps) {
			const persona = await personaRow(ctx);
			const prompt = await deps.personaPrompt(ctx.actor!.id);
			return {
				id: ctx.actor!.id,
				name: persona?.name ?? ctx.actor!.name,
				description: persona?.description ?? null,
				system_prompt: prompt?.system_prompt ?? null,
				prompt_version: prompt?.prompt_version ?? null,
				skills: [],
			};
		},
	},
	{
		name: 'get_persona_skill',
		description: "Returns one of the acting persona's skills by slug: its body and version",
		inputSchema: { type: 'object', properties: { slug: { type: 'string' } }, required: ['slug'] },
		async handler() {
			throw new ToolError('No skills in local workspaces yet');
		},
	},
	{
		name: 'list_workspaces',
		description: 'List all workspaces the authenticated user belongs to. With a persona token, only the token\'s own workspace',
		inputSchema: { type: 'object', properties: {} },
		async handler(_args, ctx) {
			return [{ id: ctx.workspace.id, name: ctx.workspace.name, code: ctx.workspace.code }];
		},
	},
	{
		name: 'get_task',
		description: 'Get a task by its id',
		permission: 'tasks:read',
		inputSchema: { type: 'object', properties: { taskId: { type: 'number' } }, required: ['taskId'] },
		async handler(args, ctx, router) {
			ensureTokenWorkspace(args, ctx);
			return runRoute(router, ctx, 'GET', `tasks/${requireArg(args, 'taskId')}`);
		},
	},
	{
		name: 'search_tasks',
		description: 'Search tasks by query string within this workspace. Paginated: page (default 1), perPage (default 50, max 100)',
		permission: 'tasks:read',
		inputSchema: {
			type: 'object',
			properties: {
				workspaceId: { type: 'number' },
				query: { type: 'string' },
				page: { type: 'number' },
				perPage: { type: 'number' },
			},
			required: ['query'],
		},
		async handler(args, ctx, router) {
			ensureTokenWorkspace(args, ctx);
			const query = requireArg(args, 'query');
			return pageOf(router, ctx, `tasks${qs({ search: query, page: args.page, per_page: perPageArg(args) })}`);
		},
	},
	{
		name: 'list_tasks_by_status',
		description: "List tasks by status TYPE (default|active|completed|hidden|archived, 'done' is an alias of archived) or a numeric status id",
		permission: 'tasks:read',
		inputSchema: {
			type: 'object',
			properties: {
				status: { type: 'string' },
				workspaceId: { type: 'number' },
				page: { type: 'number' },
				perPage: { type: 'number' },
			},
			required: ['status'],
		},
		async handler(args, ctx, router) {
			ensureTokenWorkspace(args, ctx);
			const status = String(requireArg(args, 'status'));
			const filter = /^\d+$/.test(status)
				? { status_id: status }
				: { status_type: status === 'done' ? 'archived' : status };
			return pageOf(router, ctx, `tasks${qs({ ...filter, page: args.page, per_page: perPageArg(args) })}`);
		},
	},
	{
		name: 'create_task',
		description: 'Create a new task in this workspace',
		permission: 'tasks:write',
		inputSchema: {
			type: 'object',
			properties: {
				title: { type: 'string' },
				description: { type: 'string' },
				workspaceId: { type: 'number' },
				statusId: { type: 'number' },
				projectCategoryId: { type: 'number' },
			},
			required: ['title'],
		},
		async handler(args, ctx, router) {
			ensureTokenWorkspace(args, ctx);
			const body: Record<string, any> = { title: requireArg(args, 'title') };
			if (args.description !== undefined) body.description = args.description;
			if (args.statusId !== undefined) body.status_id = args.statusId;
			if (args.projectCategoryId !== undefined) body.project_category_id = args.projectCategoryId;
			return runRoute(router, ctx, 'POST', 'tasks', body);
		},
	},
	{
		name: 'update_task',
		description: 'Update an existing task. Only provided fields are changed. A persona cannot move a task to another workspace',
		permission: 'tasks:write',
		inputSchema: {
			type: 'object',
			properties: {
				taskId: { type: 'number' },
				title: { type: 'string' },
				description: { type: 'string' },
				statusId: { type: 'number' },
			},
			required: ['taskId'],
		},
		async handler(args, ctx, router) {
			if (args.workspaceId !== undefined && args.workspaceId !== null) {
				throw new ToolError('A persona cannot move a task to another workspace');
			}
			const taskId = requireArg(args, 'taskId');
			const body: Record<string, any> = {};
			if (args.title !== undefined) body.title = args.title;
			if (args.description !== undefined) body.description = args.description;
			if (args.statusId !== undefined) body.status_id = args.statusId;
			return runRoute(router, ctx, 'PATCH', `tasks/${taskId}`, body);
		},
	},
	{
		name: 'list_comments',
		description: 'List comments on a task by taskId. Paginated: page (default 1), perPage (default 50, max 100)',
		permission: 'comments:read',
		inputSchema: {
			type: 'object',
			properties: { taskId: { type: 'number' }, page: { type: 'number' }, perPage: { type: 'number' } },
			required: ['taskId'],
		},
		async handler(args, ctx, router) {
			const all = await runRoute(router, ctx, 'GET', `tasks/${requireArg(args, 'taskId')}/comments`);
			return paginateInMemory(all, args.page, args.perPage);
		},
	},
	{
		name: 'add_comment',
		description: 'Add a comment to a task',
		permission: 'comments:write',
		inputSchema: {
			type: 'object',
			properties: { taskId: { type: 'number' }, message: { type: 'string' } },
			required: ['taskId', 'message'],
		},
		async handler(args, ctx, router) {
			const taskId = requireArg(args, 'taskId');
			const message = requireArg(args, 'message');
			return runRoute(router, ctx, 'POST', `tasks/${taskId}/comments`, { message });
		},
	},
	{
		name: 'list_task_files',
		description: 'List the files attached to a task. Paginated: page (default 1), perPage (default 50, max 100)',
		permission: 'files:attachments',
		inputSchema: {
			type: 'object',
			properties: { taskId: { type: 'number' }, page: { type: 'number' }, perPage: { type: 'number' } },
			required: ['taskId'],
		},
		async handler(args, ctx, router) {
			const rows = await runRoute(router, ctx, 'GET', `tasks/${requireArg(args, 'taskId')}/files`);
			// get_task_image isn't offered locally, so nothing is ever viewable via this MCP.
			const items = rows.map((f: any) => ({
				id: f.id,
				taskId: f.task_id,
				name: f.name,
				mimeType: f.mime_type,
				sizeBytes: f.size,
				viewable: false,
				createdAt: f.created_at,
			}));
			return paginateInMemory(items, args.page, args.perPage);
		},
	},
	{
		name: 'list_statuses',
		description: 'List all task statuses in this workspace',
		permission: 'statuses:read',
		inputSchema: { type: 'object', properties: { workspaceId: { type: 'number' } } },
		async handler(args, ctx, router) {
			ensureTokenWorkspace(args, ctx);
			return runRoute(router, ctx, 'GET', 'workspaces/statuses');
		},
	},
	{
		name: 'start_agent_work',
		description: 'Record that you, an AI agent, start working on a task. Keep the returned id for update_agent_work and finish_agent_work',
		permission: 'agent_work:write',
		inputSchema: {
			type: 'object',
			properties: {
				taskId: { type: 'number' },
				agent: { type: 'string' },
				model: { type: 'string' },
				branch: { type: 'string' },
				sessionId: { type: 'string' },
			},
			required: ['taskId', 'agent'],
		},
		async handler(args, ctx, router) {
			const taskId = requireArg(args, 'taskId');
			const body: Record<string, any> = { agent: requireArg(args, 'agent') };
			if (args.model !== undefined) body.model = args.model;
			if (args.branch !== undefined) body.branch = args.branch;
			if (args.sessionId !== undefined) body.session_id = args.sessionId;
			return runRoute(router, ctx, 'POST', `tasks/${taskId}/agent-work`, body);
		},
	},
	{
		name: 'update_agent_work',
		description: 'Report progress of your running agent work: branch, summary, pull request url, commits, test results',
		permission: 'agent_work:write',
		inputSchema: {
			type: 'object',
			properties: {
				runId: { type: 'number' },
				branch: { type: 'string' },
				summary: { type: 'string' },
				prUrl: { type: 'string' },
				commits: { type: 'array', items: { type: 'string' } },
				testsPassed: { type: 'number' },
				testsFailed: { type: 'number' },
				testsCommand: { type: 'string' },
			},
			required: ['runId'],
		},
		async handler(args, ctx, router) {
			const runId = requireArg(args, 'runId');
			return runRoute(router, ctx, 'PATCH', `agent-work/${runId}`, agentWorkBody(args, true));
		},
	},
	{
		name: 'finish_agent_work',
		description: 'Finish your agent work run: status succeeded, failed or cancelled, plus a short summary',
		permission: 'agent_work:write',
		inputSchema: {
			type: 'object',
			properties: {
				runId: { type: 'number' },
				status: { type: 'string' },
				summary: { type: 'string' },
				prUrl: { type: 'string' },
				commits: { type: 'array', items: { type: 'string' } },
				testsPassed: { type: 'number' },
				testsFailed: { type: 'number' },
				testsCommand: { type: 'string' },
			},
			required: ['runId', 'status'],
		},
		async handler(args, ctx, router) {
			const runId = requireArg(args, 'runId');
			const body = agentWorkBody(args, false);
			body.status = requireArg(args, 'status');
			return runRoute(router, ctx, 'POST', `agent-work/${runId}/finish`, body);
		},
	},
	{
		name: 'list_agent_work',
		description: 'Agent work runs on a task, newest first, with the total agent time next to the human timer',
		permission: 'agent_work:read',
		inputSchema: { type: 'object', properties: { taskId: { type: 'number' } }, required: ['taskId'] },
		async handler(args, ctx, router) {
			return runRoute(router, ctx, 'GET', `tasks/${requireArg(args, 'taskId')}/agent-work`);
		},
	},
	{
		name: 'list_my_queue',
		description: "List the tasks assigned to you (the acting persona), ordered by status then position. Optional statusType (default|active|completed|hidden|archived), statusId, limit (default 50, max 100)",
		permission: 'tasks:read',
		inputSchema: {
			type: 'object',
			properties: { statusType: { type: 'string' }, statusId: { type: 'number' }, limit: { type: 'number' } },
		},
		async handler(args, ctx, router) {
			const filter: Record<string, unknown> = { persona: ctx.actor!.id, sort: 'queue', per_page: perPageArg({ perPage: args.limit }) };
			if (args.statusId !== undefined && args.statusId !== null) filter.status_id = args.statusId;
			else if (args.statusType) filter.status_type = args.statusType === 'done' ? 'archived' : args.statusType;
			return pageOf(router, ctx, `tasks${qs(filter)}`);
		},
	},
	{
		name: 'list_assignable_personas',
		description: 'List the personas that can be assigned to tasks in this workspace',
		audience: 'user',
		inputSchema: { type: 'object', properties: { workspaceId: { type: 'number' } } },
		async handler(args, ctx, router) {
			ensureTokenWorkspace(args, ctx);
			return runRoute(router, ctx, 'GET', `workspaces/${ctx.workspace.id}/assignable-personas`);
		},
	},
	{
		name: 'assign_persona',
		description: 'Put a task into a persona queue. persona is its uuid or exact name among the assignable personas',
		audience: 'user',
		inputSchema: {
			type: 'object',
			properties: { taskId: { type: 'number' }, persona: { type: 'string' } },
			required: ['taskId', 'persona'],
		},
		async handler(args, ctx, router) {
			const persona = encodeURIComponent(String(requireArg(args, 'persona')));
			return runRoute(router, ctx, 'POST', `tasks/${requireArg(args, 'taskId')}/personas/${persona}`);
		},
	},
	{
		name: 'unassign_persona',
		description: 'Remove a persona from a task. persona is its uuid or exact name',
		audience: 'user',
		inputSchema: {
			type: 'object',
			properties: { taskId: { type: 'number' }, persona: { type: 'string' } },
			required: ['taskId', 'persona'],
		},
		async handler(args, ctx, router) {
			const persona = encodeURIComponent(String(requireArg(args, 'persona')));
			return runRoute(router, ctx, 'DELETE', `tasks/${requireArg(args, 'taskId')}/personas/${persona}`);
		},
	},
	{
		name: 'tmgr_request',
		description: 'Make an HTTP request to the local tmgr REST API. method: GET|POST|PATCH|DELETE. path: e.g. /api/tasks/1. jsonBody: optional JSON string',
		raw: true,
		inputSchema: {
			type: 'object',
			properties: { method: { type: 'string' }, path: { type: 'string' }, jsonBody: { type: 'string' } },
			required: ['method', 'path'],
		},
		async handler(args, ctx, router) {
			const method = requireArg(args, 'method');
			const path = requireArg(args, 'path');
			const withoutQuery = normalizePath(String(path));
			if (withoutQuery === 'local' || withoutQuery.startsWith('local/')) {
				throw new ToolError('local/* routes are not available via tmgr_request');
			}
			const res = await dispatchLocal(router, ctx, method, path, args.jsonBody ?? undefined);
			if (!res) throw new ToolError(`No local route for ${method} ${withoutQuery}`);
			if (res.status >= 400) throw new ToolError(res.data?.message ?? `Request failed with status ${res.status}`);
			if (res.data instanceof Blob) throw new ToolError('tmgr_request cannot return binary content');
			return JSON.stringify(res.data);
		},
	},
];

/** Mirrors `AgentWorkMcpTools.parseCommits`: each entry is `"<sha> <subject>"`, split on the first run of whitespace. */
const parseCommits = (commits: string[]): { sha: string; message: string | null }[] =>
	commits
		.filter((line) => typeof line === 'string' && line.trim())
		.map((line) => {
			const match = /^(\S+)(?:\s+([\s\S]*))?$/.exec(line.trim())!;
			return { sha: match[1], message: match[2] ?? null };
		});

const agentWorkBody = (args: Record<string, any>, includeBranch: boolean): Record<string, any> => {
	const body: Record<string, any> = {};
	if (includeBranch && args.branch !== undefined) body.branch = args.branch;
	if (args.summary !== undefined) body.summary = args.summary;
	if (args.prUrl !== undefined) body.pr_url = args.prUrl;
	if (args.commits !== undefined) body.commits = parseCommits(args.commits);
	if (args.testsPassed !== undefined || args.testsFailed !== undefined || args.testsCommand !== undefined) {
		body.tests = { passed: args.testsPassed ?? null, failed: args.testsFailed ?? null, command: args.testsCommand ?? null };
	}
	return body;
};

TOOLS.push(...PAGES_TOOLS);

const TOOLS_BY_NAME = new Map(TOOLS.map((tool) => [tool.name, tool]));

const isPersonaCtx = (ctx: LocalContext) => ctx.actor?.kind === 'persona';

const listedTools = async (ctx: LocalContext) => {
	if (!isPersonaCtx(ctx)) return TOOLS.filter((tool) => tool.audience === 'user');
	const permissions = await personaGrantPermissions(ctx);
	return TOOLS.filter(
		(tool) => tool.audience !== 'user' && (!tool.permission || permissions.includes(tool.permission)),
	);
};

const toolResultOk = (payload: any, tool: ToolDef) => ({
	content: [
		{
			type: 'text',
			text: tool.raw ? payload : tool.literal ? JSON.stringify(payload) : JSON.stringify(camelizeDeep(payload)),
		},
	],
});

const toolResultError = (message: string) => ({
	isError: true,
	content: [{ type: 'text', text: message }],
});

const handleInitialize = (params: any) => {
	const requested = params?.protocolVersion;
	const protocolVersion = SUPPORTED_PROTOCOL_VERSIONS.includes(requested) ? requested : DEFAULT_PROTOCOL_VERSION;
	return {
		protocolVersion,
		capabilities: { tools: {} },
		serverInfo: { name: 'tmgr-desktop-local', version: '1' },
		instructions: 'Local TMGR tools for this persona, scoped to its own workspace and permissions.',
	};
};

/** personaGate (via `dispatchLocal`) decides permissions; this layer never re-implements it. */
const handleToolsCall = async (params: any, ctx: LocalContext, router: LocalRouter, deps: McpDeps) => {
	try {
		const persona = isPersonaCtx(ctx);
		if (persona) await checkPersonaIdentity(ctx);
		const tool = TOOLS_BY_NAME.get(params?.name);
		if (!tool || (persona ? tool.audience === 'user' : tool.audience !== 'user')) {
			return toolResultError('Tool not available to personas');
		}
		const result = await tool.handler(params?.arguments ?? {}, ctx, router, deps);
		return toolResultOk(result, tool);
	} catch (error) {
		if (error instanceof ToolError) return toolResultError(error.message);
		return toolResultError(error instanceof Error ? error.message : String(error));
	}
};

const processMessage = async (
	msg: any,
	ctx: LocalContext,
	router: LocalRouter,
	deps: McpDeps,
): Promise<{ jsonrpc: '2.0'; id: any; result?: any; error?: { code: number; message: string } } | null> => {
	const isPlainObject = !!msg && typeof msg === 'object' && !Array.isArray(msg);
	if (!isPlainObject || msg.jsonrpc !== '2.0' || typeof msg.method !== 'string') {
		return { jsonrpc: '2.0', id: isPlainObject && 'id' in msg ? msg.id : null, error: { code: -32600, message: 'Invalid Request' } };
	}
	const isNotification = !('id' in msg);
	const respond = (result: any) => (isNotification ? null : { jsonrpc: '2.0' as const, id: msg.id, result });
	const respondError = (code: number, message: string) =>
		isNotification ? null : { jsonrpc: '2.0' as const, id: msg.id, error: { code, message } };

	switch (msg.method) {
		case 'initialize':
			return respond(handleInitialize(msg.params));
		case 'ping':
			return respond({});
		case 'tools/list':
			return respond({
				tools: (await listedTools(ctx)).map((tool) => ({
					name: tool.name,
					description: tool.description,
					inputSchema: tool.inputSchema,
				})),
			});
		case 'tools/call':
			return respond(await handleToolsCall(msg.params, ctx, router, deps));
		default:
			return respondError(-32601, `Method not found: ${msg.method}`);
	}
};

export const handleMcpRequest = async (
	router: LocalRouter,
	ctx: LocalContext,
	rawBody: string,
	deps: McpDeps,
): Promise<{ status: number; body: string }> => {
	let parsed: any;
	try {
		parsed = JSON.parse(rawBody);
	} catch {
		return {
			status: 400,
			body: JSON.stringify({ jsonrpc: '2.0', id: null, error: { code: -32700, message: 'Parse error' } }),
		};
	}
	if (Array.isArray(parsed) && parsed.length === 0) {
		return {
			status: 200,
			body: JSON.stringify({ jsonrpc: '2.0', id: null, error: { code: -32600, message: 'Invalid Request' } }),
		};
	}
	const messages = Array.isArray(parsed) ? parsed : [parsed];
	const responses = [];
	for (const msg of messages) {
		const response = await processMessage(msg, ctx, router, deps);
		if (response) responses.push(response);
	}
	if (!responses.length) return { status: 202, body: '' };
	return { status: 200, body: JSON.stringify(Array.isArray(parsed) ? responses : responses[0]) };
};
