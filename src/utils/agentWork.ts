import type { AgentWorkRun, AgentWorkTotals } from '@/actions/tmgr/agentWork';

const KNOWN_AGENTS: Record<string, string> = {
	'claude-code': 'Claude Code',
	codex: 'Codex',
	cursor: 'Cursor',
	'gemini-cli': 'Gemini CLI',
};

export const agentLabel = (agent: string): string =>
	KNOWN_AGENTS[agent] ??
	agent
		.split(/[-_\s]+/)
		.filter(Boolean)
		.map((part) => part[0].toUpperCase() + part.slice(1))
		.join(' ');

export const formatWorkDuration = (seconds: number): string => {
	const total = Math.max(0, Math.floor(seconds));
	const h = Math.floor(total / 3600);
	const m = Math.floor((total % 3600) / 60);
	const s = total % 60;
	if (h > 0) return `${h}h ${m}m`;
	if (m > 0) return `${m}m ${s}s`;
	return `${s}s`;
};

export const liveSeconds = (run: AgentWorkRun, nowMs: number): number =>
	run.status === 'running'
		? Math.max(0, Math.floor((nowMs - Date.parse(run.started_at)) / 1000))
		: run.duration_seconds;

export const liveTotals = (
	runs: AgentWorkRun[],
	totals: AgentWorkTotals,
	nowMs: number,
): { agentSeconds: number; humanSeconds: number } => ({
	agentSeconds: runs.reduce((sum, run) => sum + liveSeconds(run, nowMs), 0),
	humanSeconds: totals.human_seconds,
});

export const upsertRun = (
	runs: AgentWorkRun[],
	run: AgentWorkRun,
): AgentWorkRun[] =>
	runs.some((r) => r.id === run.id)
		? runs.map((r) => (r.id === run.id ? run : r))
		: [run, ...runs];
