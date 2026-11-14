import { dispatchLocal } from './dispatch';
import { camelizeDeep, MCP_TOOLS, type McpDeps, type ToolDef } from './mcp';
import type { LocalRouter } from './router';
import type { LocalContext } from './types';

export const LOCAL_AGENT_TOOL_LIMIT = 6000;
const TRUNCATION_NOTE = '\n[result truncated — narrow your query]';

type Args = Record<string, any>;

export interface LocalAgentTool {
	name: string;
	description: string;
	inputSchema: ToolDef['inputSchema'];
	write: boolean;
	run: (
		args: Args,
		ctx: LocalContext,
		router: LocalRouter,
		deps: McpDeps,
	) => Promise<string>;
}

const REUSED_READ = [
	'workspace_context',
	'pages_for_task',
	'pages_search',
	'pages_tree',
	'pages_get',
	'get_task',
	'search_tasks',
	'list_tasks_by_status',
	'list_comments',
	'list_task_files',
	'list_statuses',
];
const REUSED_WRITE = [
	'pages_create',
	'pages_update',
	'pages_append',
	'pages_set_section',
	'create_task',
	'update_task',
	'add_comment',
];

const reused = (name: string, write: boolean): LocalAgentTool => {
	const tool = MCP_TOOLS.find((t) => t.name === name);
	if (!tool) throw new Error(`MCP tool ${name} is missing`);
	return {
		name,
		description: tool.description,
		inputSchema: tool.inputSchema,
		write,
		run: async (args, ctx, router, deps) => {
			const result = await tool.handler(args, ctx, router, deps);
			return tool.raw
				? String(result)
				: JSON.stringify(tool.literal ? result : camelizeDeep(result));
		},
	};
};

const route = async (
	router: LocalRouter,
	ctx: LocalContext,
	method: string,
	path: string,
	body?: unknown,
) => {
	const res = await dispatchLocal(router, ctx, method, path, body);
	if (!res) throw new Error(`No local route for ${method} ${path}`);
	if (res.status >= 400)
		throw new Error(
			res.data?.message ?? `Request failed with status ${res.status}`,
		);
	return res;
};

const need = (args: Args, key: string) => {
	if (args[key] === undefined || args[key] === null || args[key] === '')
		throw new Error(`${key} is required`);
	return args[key];
};

const needId = (args: Args, key: string) => {
	const id = Number(need(args, key));
	if (!Number.isInteger(id) || id <= 0)
		throw new Error(`${key} must be a positive integer`);
	return id;
};

const checkWorkspace = (args: Args, ctx: LocalContext) => {
	if (args.workspaceId != null && Number(args.workspaceId) !== ctx.workspace.id)
		throw new Error('workspaceId does not match this local workspace');
};

const json = (value: unknown) => JSON.stringify(camelizeDeep(value));
const taskIdProp = {
	type: 'object' as const,
	properties: { taskId: { type: 'number', description: 'Task id' } },
	required: ['taskId'],
};
const PAGE_DEFAULT = 50;
const PAGE_MAX = 100;

const NEW_TOOLS: LocalAgentTool[] = [
	{
		name: 'list_tasks',
		description:
			"List current user's tasks in their default workspace. Paginated: page (default 1), perPage (default 50, max 100); `hasMore` says whether to fetch the next page",
		inputSchema: {
			type: 'object',
			properties: {
				page: {
					type: 'number',
					description: 'Page number, 1-based (default 1)',
				},
				perPage: {
					type: 'number',
					description: 'Rows per page (default 50, max 100)',
				},
			},
		},
		write: false,
		run: async (args, ctx, router) => {
			const perPage =
				!args.perPage || args.perPage < 1
					? PAGE_DEFAULT
					: Math.min(Number(args.perPage), PAGE_MAX);
			const query = new URLSearchParams({ per_page: String(perPage) });
			if (args.page) query.set('page', String(args.page));
			const res = await route(router, ctx, 'GET', `tasks?${query}`);
			const meta = res.data.meta;
			return json({
				items: res.data.data,
				page: meta.current_page,
				perPage: meta.per_page,
				total: meta.total,
				hasMore: meta.current_page < meta.last_page,
			});
		},
	},
	{
		name: 'start_task_timer',
		description:
			"Start the HUMAN's own time tracking on a task, only when the user asks you to track their time. Idempotent: a timer that is already running is left as is. Returns the task with startTime",
		inputSchema: taskIdProp,
		write: true,
		run: async (args, ctx, router) =>
			json(
				(
					await route(
						router,
						ctx,
						'POST',
						`tasks/${needId(args, 'taskId')}/countdown`,
					)
				).data.data,
			),
	},
	{
		name: 'stop_task_timer',
		description:
			"Stop the HUMAN's own time tracking on a task. Elapsed seconds are added to commonTime. Idempotent on an idle timer",
		inputSchema: taskIdProp,
		write: true,
		run: async (args, ctx, router) =>
			json(
				(
					await route(
						router,
						ctx,
						'DELETE',
						`tasks/${needId(args, 'taskId')}/countdown`,
					)
				).data.data,
			),
	},
	{
		name: 'create_status',
		description:
			'Create a new task status in a workspace. color should be a hex value, e.g. #4CAF50',
		inputSchema: {
			type: 'object',
			properties: {
				workspaceId: {
					type: 'number',
					description: 'Workspace id; omit for this workspace',
				},
				name: { type: 'string' },
				color: { type: 'string' },
			},
			required: ['name'],
		},
		write: true,
		run: async (args, ctx, router) => {
			checkWorkspace(args, ctx);
			const body: Args = { name: need(args, 'name') };
			if (args.color !== undefined) body.color = args.color;
			return json(
				(
					await route(
						router,
						ctx,
						'POST',
						`workspaces/${ctx.workspace.id}/statuses`,
						body,
					)
				).data.data,
			);
		},
	},
	{
		name: 'update_status',
		description: 'Update an existing status. Only non-null fields are changed',
		inputSchema: {
			type: 'object',
			properties: {
				statusId: { type: 'number' },
				name: { type: 'string' },
				color: { type: 'string' },
			},
			required: ['statusId'],
		},
		write: true,
		run: async (args, ctx, router) => {
			const body: Args = {};
			if (args.name !== undefined) body.name = args.name;
			if (args.color !== undefined) body.color = args.color;
			return json(
				(
					await route(
						router,
						ctx,
						'PUT',
						`statuses/${needId(args, 'statusId')}`,
						body,
					)
				).data.data,
			);
		},
	},
];

export const localAgentTools: LocalAgentTool[] = [
	...REUSED_READ.map((name) => reused(name, false)),
	...REUSED_WRITE.map((name) => reused(name, true)),
	...NEW_TOOLS,
];

const BY_NAME = new Map(localAgentTools.map((tool) => [tool.name, tool]));

export const isWriteTool = (name: string): boolean =>
	BY_NAME.get(name)?.write === true;

export const toOpenAiTools = () =>
	localAgentTools.map((tool) => ({
		type: 'function' as const,
		function: {
			name: tool.name,
			description: tool.description,
			parameters: tool.inputSchema,
		},
	}));

export const runLocalTool = async (
	name: string,
	args: unknown,
	ctx: LocalContext,
	router: LocalRouter,
	deps: McpDeps,
): Promise<string> => {
	const tool = BY_NAME.get(name);
	if (!tool) return `Unknown tool: ${name}`;
	const safeArgs: Args =
		args && typeof args === 'object' && !Array.isArray(args)
			? (args as Args)
			: {};
	let text: string;
	try {
		text = await tool.run(safeArgs, ctx, router, deps);
	} catch (error) {
		text = `Error: ${error instanceof Error ? error.message : String(error)}`;
	}
	return text.length > LOCAL_AGENT_TOOL_LIMIT
		? text.slice(0, LOCAL_AGENT_TOOL_LIMIT) + TRUNCATION_NOTE
		: text;
};
