import { formatWorkDuration, isHttpUrl } from '../utils/agentWork';
import type {
	HookStat,
	McpStat,
	NamedCount,
	TaskContext,
	TaskNode,
	TaskTokens,
	TaskToolStat,
} from './taskUsage';

const usdFormatter = new Intl.NumberFormat('en-US', {
	style: 'currency',
	currency: 'USD',
	minimumFractionDigits: 2,
	maximumFractionDigits: 2,
});

export const formatUsd = (n: number): string => {
	if (!Number.isFinite(n)) return '—';
	if (n > 0 && n < 0.01) return '<$0.01';
	return usdFormatter.format(n);
};

export const formatUsdString = (
	s: string | null | undefined,
): string | null => {
	if (s === null || s === undefined || s.trim() === '') return null;
	const n = Number(s);
	return Number.isFinite(n) ? formatUsd(n) : null;
};

export const formatTokens = (n: number): string => {
	if (!Number.isFinite(n)) return '—';
	const abs = Math.abs(n);
	if (abs >= 1e6) return `${trimDecimal(n / 1e6)}M`;
	if (abs >= 1e3) return `${trimDecimal(n / 1e3)}k`;
	return String(Math.round(n));
};

const trimDecimal = (n: number): string => {
	const text = Math.abs(n) >= 100 ? n.toFixed(0) : n.toFixed(1);
	return text.replace(/\.0$/, '');
};

export const formatMs = (ms: number): string => formatWorkDuration(ms / 1000);

export const cacheReadPct = (tokens: TaskTokens): number | null => {
	const denominator =
		tokens.input +
		tokens.cache_read +
		tokens.cache_write_5m +
		tokens.cache_write_1h;
	if (denominator === 0) return null;
	return Math.round((tokens.cache_read / denominator) * 100);
};

export interface TokenRow {
	key: 'input' | 'cache_read' | 'cache_write' | 'output' | 'reasoning';
	label: string;
	value: number;
	pct?: number | null;
}

export const tokenRows = (tokens: TaskTokens): TokenRow[] => {
	const cacheWrite = tokens.cache_write_5m + tokens.cache_write_1h;
	const rows: TokenRow[] = [
		{ key: 'input', label: 'Input', value: tokens.input },
		{
			key: 'cache_read',
			label: 'Cache read',
			value: tokens.cache_read,
			pct: cacheReadPct(tokens),
		},
		{ key: 'cache_write', label: 'Cache write', value: cacheWrite },
		{ key: 'output', label: 'Output', value: tokens.output },
		{ key: 'reasoning', label: 'Reasoning', value: tokens.reasoning },
	];
	return rows.filter(
		(row) => row.key === 'input' || row.key === 'output' || row.value !== 0,
	);
};

export const nodeLabel = (node: TaskNode): string => {
	if (node.tool === 'codex') return 'Codex';
	if (node.tool === 'claude' && node.agent_type === 'main')
		return 'Claude Code';
	return node.agent_type;
};

export const modelShort = (model: string): string =>
	model.replace(/^claude-/, '');

export interface TreeNode {
	node: TaskNode;
	children: TreeNode[];
	depth: number;
}

const compareNodes = (a: TaskNode, b: TaskNode): number => {
	if (a.started_at !== b.started_at)
		return a.started_at < b.started_at ? -1 : 1;
	if (a.id === b.id) return 0;
	return a.id < b.id ? -1 : 1;
};

export const buildTree = (nodes: TaskNode[]): TreeNode[] => {
	const byId = new Map(nodes.map((node) => [node.id, node]));
	const childrenOf = new Map<string, TaskNode[]>();
	const roots: TaskNode[] = [];
	for (const node of nodes) {
		if (node.parent_id !== null && byId.has(node.parent_id)) {
			const siblings = childrenOf.get(node.parent_id) ?? [];
			siblings.push(node);
			childrenOf.set(node.parent_id, siblings);
		} else {
			roots.push(node);
		}
	}

	const visited = new Set<string>();
	const build = (node: TaskNode, depth: number): TreeNode | null => {
		if (visited.has(node.id)) return null;
		visited.add(node.id);
		const children = (childrenOf.get(node.id) ?? [])
			.slice()
			.sort(compareNodes)
			.map((child) => build(child, depth + 1))
			.filter((child): child is TreeNode => child !== null);
		return { node, children, depth };
	};

	return roots
		.slice()
		.sort(compareNodes)
		.map((root) => build(root, 0))
		.filter((root): root is TreeNode => root !== null);
};

export interface BlockSummaryInput {
	youSeconds: number | null;
	agentSeconds: number;
	aiCostUsd: number | null;
	prLabel: string | null;
}

export const blockSummary = ({
	youSeconds,
	agentSeconds,
	aiCostUsd,
	prLabel,
}: BlockSummaryInput): string => {
	const parts: string[] = [];
	if (youSeconds !== null) parts.push(`You ${formatWorkDuration(youSeconds)}`);
	if (agentSeconds > 0)
		parts.push(`Agents ${formatWorkDuration(agentSeconds)}`);
	if (aiCostUsd !== null) parts.push(`${formatUsd(aiCostUsd)} AI`);
	if (prLabel !== null) parts.push(prLabel);
	return parts.join(' · ');
};

export const prLabel = (
	prs: { repo: string; number: number }[],
	runPrUrls: (string | null)[],
): string | null => {
	if (prs.length > 0) return `PR #${prs[0].number}`;
	const url = runPrUrls.find(isHttpUrl);
	if (!url) return null;
	const match = /\/pull\/(\d+)/.exec(url);
	return match ? `PR #${match[1]}` : null;
};

export interface ToolTotal {
	name: string;
	count: number;
	errors: number;
}

const byCountThenName =
	<T extends { count: number }>(nameOf: (item: T) => string) =>
	(a: T, b: T): number => {
		if (a.count !== b.count) return b.count - a.count;
		const left = nameOf(a);
		const right = nameOf(b);
		return left < right ? -1 : left > right ? 1 : 0;
	};

export const toolTotals = (
	tools: TaskToolStat[],
	limit?: number,
): ToolTotal[] => {
	const totals = new Map<string, ToolTotal>();
	for (const tool of tools) {
		const total = totals.get(tool.name) ?? {
			name: tool.name,
			count: 0,
			errors: 0,
		};
		total.count += tool.count;
		total.errors += tool.errors;
		totals.set(tool.name, total);
	}
	const sorted = [...totals.values()].sort(byCountThenName((t) => t.name));
	return limit === undefined ? sorted : sorted.slice(0, limit);
};

export const formatTool = (tool: ToolTotal): string =>
	`${tool.name} ×${tool.count}`;

export interface McpServerGroup {
	server: string;
	calls: number;
	errors: number;
	tools: { tool: string; count: number }[];
}

export const groupMcp = (mcp: McpStat[]): McpServerGroup[] => {
	const groups = new Map<string, McpServerGroup>();
	const toolCounts = new Map<string, Map<string, number>>();
	for (const stat of mcp) {
		const group = groups.get(stat.server) ?? {
			server: stat.server,
			calls: 0,
			errors: 0,
			tools: [],
		};
		group.calls += stat.count;
		group.errors += stat.errors;
		groups.set(stat.server, group);
		const counts = toolCounts.get(stat.server) ?? new Map<string, number>();
		counts.set(stat.tool, (counts.get(stat.tool) ?? 0) + stat.count);
		toolCounts.set(stat.server, counts);
	}
	return [...groups.values()]
		.map((group) => ({
			...group,
			tools: [...(toolCounts.get(group.server) ?? new Map()).entries()]
				.map(([tool, count]) => ({ tool, count }))
				.sort(byCountThenName((t) => t.tool)),
		}))
		.sort(
			(a, b) =>
				b.calls - a.calls ||
				(a.server < b.server ? -1 : a.server > b.server ? 1 : 0),
		);
};

export interface HookGroup {
	event: string;
	name: string;
	count: number;
	errors: number;
	duration_ms: number;
}

export const groupHooks = (hooks: HookStat[]): HookGroup[] => {
	const groups = new Map<string, HookGroup>();
	for (const hook of hooks) {
		const key = `${hook.event}\u0000${hook.name}`;
		const group = groups.get(key) ?? {
			event: hook.event,
			name: hook.name,
			count: 0,
			errors: 0,
			duration_ms: 0,
		};
		group.count += hook.count;
		group.errors += hook.errors;
		group.duration_ms += hook.duration_ms;
		groups.set(key, group);
	}
	return [...groups.values()].sort(
		(a, b) =>
			b.duration_ms - a.duration_ms ||
			(a.event < b.event ? -1 : a.event > b.event ? 1 : 0) ||
			(a.name < b.name ? -1 : a.name > b.name ? 1 : 0),
	);
};

export interface ContextSummary {
	skills?: { items: NamedCount[]; available: number };
	plugins?: NamedCount[];
	slash_commands?: NamedCount[];
	permission_modes?: { mode: string; count: number }[];
	compactions?: number;
	away?: number;
}

export const contextSummary = (context: TaskContext): ContextSummary => {
	const summary: ContextSummary = {};
	if (context.skills.length > 0 || context.skills_available > 0) {
		summary.skills = {
			items: context.skills,
			available: context.skills_available,
		};
	}
	if (context.plugins.length > 0) summary.plugins = context.plugins;
	if (context.slash_commands.length > 0)
		summary.slash_commands = context.slash_commands;
	if (context.permission_modes.length > 0)
		summary.permission_modes = context.permission_modes;
	if (context.compactions > 0) summary.compactions = context.compactions;
	if (context.away > 0) summary.away = context.away;
	return summary;
};
