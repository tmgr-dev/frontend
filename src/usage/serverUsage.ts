import type {
	TaskCost,
	TaskCostNode,
	TaskCostRates,
	TaskCostTokens,
} from '@/actions/tmgr/taskCost';
import { formatUsd, modelShort, pricedCost } from './taskModel';
import type {
	TaskNode,
	TaskTokens,
	TaskToolStat,
	TaskUsageAggregate,
} from './taskUsage';

type ServerAi = TaskCost['ai'];

const num = (value: unknown): number =>
	typeof value === 'number' && Number.isFinite(value) ? value : 0;

const decimal = (value: unknown): number => {
	const n = Number(value);
	return value === null || value === undefined || !Number.isFinite(n) ? 0 : n;
};

const mapTokens = (t: TaskCostTokens | null | undefined): TaskTokens => ({
	input: num(t?.input),
	output: num(t?.output),
	cache_read: num(t?.cacheRead ?? t?.cache_read),
	cache_write_5m: num(t?.cache_write_5m),
	cache_write_1h: num(t?.cache_write_1h),
	reasoning: num(t?.reasoning),
});

const mapTools = (
	tools: { name: string; count: number; errors: number }[] | null | undefined,
): TaskToolStat[] =>
	(tools ?? []).map((tool) => ({
		name: tool.name,
		count: num(tool.count),
		errors: num(tool.errors),
		duration_ms: 0,
	}));

const mapNode = (n: TaskCostNode): TaskNode => ({
	id: n.id,
	parent_id: n.parentId ?? n.parent_id ?? null,
	depth: n.depth ?? 0,
	tool: n.tool === 'codex' ? 'codex' : 'claude',
	agent_type: n.agentType ?? n.agent_type ?? 'main',
	model: n.model ?? '',
	origin: n.origin ?? null,
	link: n.link === 'exact' || n.link === 'likely' ? n.link : null,
	started_at: n.startedAt ?? n.started_at ?? '',
	ended_at: n.endedAt ?? n.ended_at ?? '',
	duration_ms: num(n.durationMs ?? n.duration_ms),
	active_ms: n.activeMs ?? n.active_ms ?? null,
	role: n.role ?? null,
	cost_usd: decimal(n.costUsd ?? n.cost_usd),
	events: num(n.events),
	tokens: mapTokens(n.tokens),
	tools: mapTools(n.tools),
});

export const serverAggregate = (
	ai: ServerAi | null | undefined,
): TaskUsageAggregate | null => {
	if (!ai?.synced) return null;
	return {
		totals: {
			cost_usd: decimal(ai.costUsd),
			events: num(ai.events),
			unpriced_events: num(ai.unpricedEvents),
			tokens: mapTokens(ai.tokens),
		},
		nodes: (ai.tree ?? []).map(mapNode),
		context: {
			tools: mapTools(ai.tools),
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
		},
		truncated: { nodes: !!ai.treeTruncated, lists: false },
	} as unknown as TaskUsageAggregate;
};

export interface UsagePick {
	source: 'synced' | 'local' | 'none';
	aggregate: TaskUsageAggregate | null;
	notice: string | null;
}

const UNSYNCED_DELTA_CENTS = 1;

export const pickUsage = ({
	synced,
	local,
}: {
	synced: TaskUsageAggregate | null;
	local: TaskUsageAggregate | null;
}): UsagePick => {
	const hasLocal = !!local && local.totals.events > 0;
	if (synced) {
		const localCost = hasLocal ? pricedCost(local) : null;
		const delta =
			localCost === null
				? 0
				: Math.round((localCost - synced.totals.cost_usd) * 1e6) / 1e6;
		return {
			source: 'synced',
			aggregate: synced,
			notice:
				Math.round(delta * 100) > UNSYNCED_DELTA_CENTS
					? `This device has ${formatUsd(delta)} of usage not yet synced.`
					: null,
		};
	}
	if (hasLocal) {
		return {
			source: 'local',
			aggregate: local,
			notice:
				'Local only — not synced. The task cost API and other members see no AI usage for this task. Run `tmgr usage sync` to upload it.',
		};
	}
	return { source: 'none', aggregate: null, notice: null };
};

const rate = (value: string | undefined): string | null => {
	if (value === undefined || value === null || String(value).trim() === '')
		return null;
	const n = Number(value);
	if (!Number.isFinite(n)) return null;
	const text = n.toFixed(6).replace(/0+$/, '');
	const [whole, fraction = ''] = text.split('.');
	return `$${whole}.${fraction.padEnd(2, '0')}`;
};

export const ratesRows = (
	rates: TaskCostRates[] | null | undefined,
): { model: string; text: string }[] =>
	(rates ?? []).map((r) => {
		const parts = [
			['input', rate(r.input)],
			['output', rate(r.output)],
			['cache read', rate(r.cache_read ?? r.cacheRead)],
			['cache write 5m', rate(r.cache_write_5m)],
			['cache write 1h', rate(r.cache_write_1h)],
		]
			.filter(([, value]) => value !== null)
			.map(([label, value]) => `${label} ${value}`);
		return {
			model: modelShort(r.model),
			text: `${parts.join(' · ')} per 1M tokens`,
		};
	});

export const unpricedWarning = (
	count: number | null | undefined,
): string | null => {
	if (!count || count <= 0) return null;
	return `${count} ${
		count === 1 ? 'event has' : 'events have'
	} no price; the cost is understated.`;
};
