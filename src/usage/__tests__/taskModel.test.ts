import {
	blockSummary,
	buildTree,
	cacheReadPct,
	contextSummary,
	formatMs,
	formatTokens,
	formatTool,
	formatUsd,
	formatUsdString,
	groupHooks,
	groupMcp,
	modelShort,
	nodeLabel,
	prLabel,
	tokenRows,
	toolTotals,
} from '../taskModel';
import type {
	TaskContext,
	TaskNode,
	TaskTokens,
	TaskToolStat,
} from '../taskUsage';

const zeroTokens: TaskTokens = {
	input: 0,
	output: 0,
	cache_read: 0,
	cache_write_5m: 0,
	cache_write_1h: 0,
	reasoning: 0,
};

const makeNode = (overrides: Partial<TaskNode> & { id: string }): TaskNode => ({
	parent_id: null,
	depth: 0,
	tool: 'claude',
	agent_type: 'main',
	model: 'claude-sonnet-4-5',
	origin: null,
	link: null,
	started_at: '2026-10-01T10:00:00Z',
	ended_at: '2026-10-01T10:05:00Z',
	duration_ms: 300_000,
	cost_usd: 0,
	events: 0,
	tokens: zeroTokens,
	tools: [],
	...overrides,
});

const makeContext = (overrides: Partial<TaskContext> = {}): TaskContext => ({
	tools: [],
	skills: [],
	skills_available: 0,
	plugins: [],
	mcp: [],
	hooks: [],
	slash_commands: [],
	permission_modes: [],
	compactions: 0,
	away: 0,
	file_edits: { count: 0, extensions: [] },
	prs: [],
	agents: [],
	...overrides,
});

describe('money', () => {
	it('formats USD with two decimals', () => {
		expect(formatUsd(18.4)).toBe('$18.40');
		expect(formatUsd(1234.5)).toBe('$1,234.50');
		expect(formatUsd(0)).toBe('$0.00');
	});

	it('marks sub-cent positive amounts', () => {
		expect(formatUsd(0.004)).toBe('<$0.01');
		expect(formatUsd(0.01)).toBe('$0.01');
	});

	it('parses server decimal strings', () => {
		expect(formatUsdString('18.4')).toBe('$18.40');
		expect(formatUsdString('0')).toBe('$0.00');
		expect(formatUsdString('0.001')).toBe('<$0.01');
		expect(formatUsdString(null)).toBeNull();
		expect(formatUsdString(undefined)).toBeNull();
		expect(formatUsdString('')).toBeNull();
		expect(formatUsdString('abc')).toBeNull();
	});
});

describe('tokens and durations', () => {
	it('formats token counts with k and M suffixes', () => {
		expect(formatTokens(0)).toBe('0');
		expect(formatTokens(999)).toBe('999');
		expect(formatTokens(1234)).toBe('1.2k');
		expect(formatTokens(1000)).toBe('1k');
		expect(formatTokens(18_400_000)).toBe('18.4M');
		expect(formatTokens(250_000_000)).toBe('250M');
	});

	it('formats milliseconds like formatWorkDuration', () => {
		expect(formatMs(0)).toBe('0s');
		expect(formatMs(8_000)).toBe('8s');
		expect(formatMs(4 * 60_000 + 12_000)).toBe('4m 12s');
		expect(formatMs(90 * 60_000)).toBe('1h 30m');
	});
});

describe('cacheReadPct', () => {
	it('is the cache read share of all input-side tokens', () => {
		expect(
			cacheReadPct({
				...zeroTokens,
				input: 100,
				cache_read: 700,
				cache_write_5m: 100,
				cache_write_1h: 100,
			}),
		).toBe(70);
	});

	it('rounds to an integer', () => {
		expect(cacheReadPct({ ...zeroTokens, input: 2, cache_read: 1 })).toBe(33);
	});

	it('is null when there is no input-side token', () => {
		expect(cacheReadPct(zeroTokens)).toBeNull();
		expect(cacheReadPct({ ...zeroTokens, output: 50 })).toBeNull();
	});
});

describe('tokenRows', () => {
	it('shows input and output even when zero, and omits other zero rows', () => {
		expect(tokenRows(zeroTokens)).toEqual([
			{ key: 'input', label: 'Input', value: 0 },
			{ key: 'output', label: 'Output', value: 0 },
		]);
	});

	it('orders rows and sums both cache write windows', () => {
		const rows = tokenRows({
			input: 10,
			output: 20,
			cache_read: 300,
			cache_write_5m: 40,
			cache_write_1h: 2,
			reasoning: 5,
		});
		expect(rows).toEqual([
			{ key: 'input', label: 'Input', value: 10 },
			{ key: 'cache_read', label: 'Cache read', value: 300, pct: 85 },
			{ key: 'cache_write', label: 'Cache write', value: 42 },
			{ key: 'output', label: 'Output', value: 20 },
			{ key: 'reasoning', label: 'Reasoning', value: 5 },
		]);
	});

	it('computes the cache read pct from input-side tokens only', () => {
		const rows = tokenRows({ ...zeroTokens, output: 1, cache_read: 9 });
		expect(rows.find((row) => row.key === 'cache_read')).toEqual({
			key: 'cache_read',
			label: 'Cache read',
			value: 9,
			pct: 100,
		});
		const empty = tokenRows({ ...zeroTokens, reasoning: 3 });
		expect(empty.map((row) => row.key)).toEqual([
			'input',
			'output',
			'reasoning',
		]);
	});
});

describe('node labels', () => {
	it('names the main Claude Code agent and Codex', () => {
		expect(nodeLabel(makeNode({ id: 'a' }))).toBe('Claude Code');
		expect(
			nodeLabel(makeNode({ id: 'b', tool: 'codex', agent_type: 'x' })),
		).toBe('Codex');
	});

	it('falls back to agent_type for sub-agents', () => {
		expect(nodeLabel(makeNode({ id: 'c', agent_type: 'Explore' }))).toBe(
			'Explore',
		);
	});

	it('strips the claude- prefix from models', () => {
		expect(modelShort('claude-opus-4-1')).toBe('opus-4-1');
		expect(modelShort('gpt-5')).toBe('gpt-5');
		expect(modelShort('')).toBe('');
	});
});

describe('buildTree', () => {
	it('returns no roots for empty input', () => {
		expect(buildTree([])).toEqual([]);
	});

	it('nests children under parents, sorted by start time then id', () => {
		const root = makeNode({ id: 'root' });
		const late = makeNode({
			id: 'late',
			parent_id: 'root',
			started_at: '2026-10-01T10:03:00Z',
		});
		const earlyB = makeNode({
			id: 'b',
			parent_id: 'root',
			started_at: '2026-10-01T10:01:00Z',
		});
		const earlyA = makeNode({
			id: 'a',
			parent_id: 'root',
			started_at: '2026-10-01T10:01:00Z',
		});
		const tree = buildTree([late, root, earlyB, earlyA]);
		expect(tree).toHaveLength(1);
		expect(tree[0].depth).toBe(0);
		expect(tree[0].children.map((c) => c.node.id)).toEqual(['a', 'b', 'late']);
		expect(tree[0].children.every((c) => c.depth === 1)).toBe(true);
	});

	it('treats nodes with a missing parent as roots', () => {
		const orphan = makeNode({ id: 'orphan', parent_id: 'gone' });
		const tree = buildTree([orphan]);
		expect(tree.map((t) => t.node.id)).toEqual(['orphan']);
		expect(tree[0].depth).toBe(0);
	});

	it('builds deeper levels with increasing depth', () => {
		const tree = buildTree([
			makeNode({ id: 'r' }),
			makeNode({ id: 'c', parent_id: 'r' }),
			makeNode({ id: 'g', parent_id: 'c' }),
		]);
		expect(tree[0].children[0].children[0].node.id).toBe('g');
		expect(tree[0].children[0].children[0].depth).toBe(2);
	});

	it('skips a node that would be reached twice', () => {
		const tree = buildTree([
			makeNode({ id: 'r' }),
			makeNode({ id: 'x', parent_id: 'r' }),
			makeNode({ id: 'x2', parent_id: 'x' }),
		]);
		expect(tree).toHaveLength(1);
		const ids: string[] = [];
		const walk = (nodes: typeof tree): void => {
			for (const n of nodes) {
				ids.push(n.node.id);
				walk(n.children);
			}
		};
		walk(tree);
		expect(ids).toEqual(['r', 'x', 'x2']);
	});

	it('does not loop forever on cycles', () => {
		const tree = buildTree([
			makeNode({ id: 'root' }),
			makeNode({ id: 'a', parent_id: 'b' }),
			makeNode({ id: 'b', parent_id: 'a' }),
			makeNode({ id: 'self', parent_id: 'self' }),
		]);
		expect(tree.map((t) => t.node.id)).toEqual(['root']);
	});

	it('preserves the link field on tree nodes', () => {
		const tree = buildTree([
			makeNode({ id: 'r' }),
			makeNode({ id: 'c', parent_id: 'r', link: 'likely' }),
		]);
		expect(tree[0].children[0].node.link).toBe('likely');
	});
});

describe('blockSummary', () => {
	it('joins all parts with a middle dot', () => {
		expect(
			blockSummary({
				youSeconds: 5400,
				agentSeconds: 9300,
				aiCostUsd: 18.4,
				prLabel: 'PR #497',
			}),
		).toBe('You 1h 30m · Agents 2h 35m · $18.40 AI · PR #497');
	});

	it('omits null parts and zero agents', () => {
		expect(
			blockSummary({
				youSeconds: null,
				agentSeconds: 0,
				aiCostUsd: 0,
				prLabel: null,
			}),
		).toBe('$0.00 AI');
		expect(
			blockSummary({
				youSeconds: 30,
				agentSeconds: 0,
				aiCostUsd: null,
				prLabel: null,
			}),
		).toBe('You 30s');
	});

	it('returns an empty string when every part is omitted', () => {
		expect(
			blockSummary({
				youSeconds: null,
				agentSeconds: 0,
				aiCostUsd: null,
				prLabel: null,
			}),
		).toBe('');
	});
});

describe('prLabel', () => {
	it('prefers the first aggregate PR', () => {
		expect(
			prLabel(
				[
					{ repo: 'tmgr', number: 497 },
					{ repo: 'tmgr', number: 12 },
				],
				['https://github.com/o/r/pull/3'],
			),
		).toBe('PR #497');
	});

	it('parses the PR number from a run url', () => {
		expect(prLabel([], [null, 'https://github.com/o/r/pull/88/files'])).toBe(
			'PR #88',
		);
	});

	it('returns null without a PR number', () => {
		expect(prLabel([], [])).toBeNull();
		expect(prLabel([], [null, 'not a url'])).toBeNull();
		expect(prLabel([], ['https://example.com/issues/5'])).toBeNull();
	});
});

describe('toolTotals', () => {
	const tools: TaskToolStat[] = [
		{ name: 'Read', count: 3, errors: 0, duration_ms: 10 },
		{ name: 'Bash', count: 42, errors: 2, duration_ms: 500 },
		{ name: 'Read', count: 2, errors: 1, duration_ms: 5 },
		{ name: 'Edit', count: 3, errors: 0, duration_ms: 1 },
	];

	it('sums per name and sorts by count then name', () => {
		expect(toolTotals(tools)).toEqual([
			{ name: 'Bash', count: 42, errors: 2 },
			{ name: 'Read', count: 5, errors: 1 },
			{ name: 'Edit', count: 3, errors: 0 },
		]);
	});

	it('applies the limit', () => {
		expect(toolTotals(tools, 1).map((t) => t.name)).toEqual(['Bash']);
	});

	it('returns an empty list for no tools', () => {
		expect(toolTotals([])).toEqual([]);
	});

	it('formats a tool total', () => {
		expect(formatTool({ name: 'Bash', count: 42, errors: 0 })).toBe('Bash ×42');
	});
});

describe('groupMcp', () => {
	it('groups by server, sorts by calls, and sorts tools by count', () => {
		expect(
			groupMcp([
				{ server: 'tmgr', tool: 'get_task', count: 2, errors: 0 },
				{ server: 'exa', tool: 'search', count: 9, errors: 1 },
				{ server: 'tmgr', tool: 'list_tasks', count: 5, errors: 1 },
				{ server: 'tmgr', tool: 'get_task', count: 1, errors: 0 },
			]),
		).toEqual([
			{
				server: 'exa',
				calls: 9,
				errors: 1,
				tools: [{ tool: 'search', count: 9 }],
			},
			{
				server: 'tmgr',
				calls: 8,
				errors: 1,
				tools: [
					{ tool: 'list_tasks', count: 5 },
					{ tool: 'get_task', count: 3 },
				],
			},
		]);
	});

	it('returns an empty list for no MCP stats', () => {
		expect(groupMcp([])).toEqual([]);
	});
});

describe('groupHooks', () => {
	it('groups by event and name and sorts by duration', () => {
		expect(
			groupHooks([
				{
					event: 'PreToolUse',
					name: 'lint',
					tool: 'Bash',
					count: 1,
					errors: 0,
					duration_ms: 100,
				},
				{
					event: 'PreToolUse',
					name: 'lint',
					tool: 'Edit',
					count: 2,
					errors: 1,
					duration_ms: 50,
				},
				{
					event: 'Stop',
					name: 'notify',
					tool: null,
					count: 1,
					errors: 0,
					duration_ms: 900,
				},
			]),
		).toEqual([
			{ event: 'Stop', name: 'notify', count: 1, errors: 0, duration_ms: 900 },
			{
				event: 'PreToolUse',
				name: 'lint',
				count: 3,
				errors: 1,
				duration_ms: 150,
			},
		]);
	});

	it('returns an empty list for no hooks', () => {
		expect(groupHooks([])).toEqual([]);
	});
});

describe('contextSummary', () => {
	it('returns an empty object when every section is empty', () => {
		expect(contextSummary(makeContext())).toEqual({});
	});

	it('includes only non-empty sections', () => {
		expect(
			contextSummary(
				makeContext({
					skills: [{ name: 'pdf', count: 2 }],
					skills_available: 14,
					compactions: 1,
					permission_modes: [{ mode: 'plan', count: 3 }],
				}),
			),
		).toEqual({
			skills: { items: [{ name: 'pdf', count: 2 }], available: 14 },
			permission_modes: [{ mode: 'plan', count: 3 }],
			compactions: 1,
		});
	});

	it('keeps skills when only the available count is set', () => {
		expect(contextSummary(makeContext({ skills_available: 4 }))).toEqual({
			skills: { items: [], available: 4 },
		});
	});

	it('includes plugins, slash commands and away time when present', () => {
		expect(
			contextSummary(
				makeContext({
					plugins: [{ name: 'superpowers', count: 1 }],
					slash_commands: [{ name: '/clear', count: 2 }],
					away: 3,
				}),
			),
		).toEqual({
			plugins: [{ name: 'superpowers', count: 1 }],
			slash_commands: [{ name: '/clear', count: 2 }],
			away: 3,
		});
	});
});
