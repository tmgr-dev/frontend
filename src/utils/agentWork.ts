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

const elapsedSince = (fromMs: number, nowMs: number): number =>
	Math.max(0, Math.floor((nowMs - fromMs) / 1000));

/**
 * Running runs count on from the server-reported duration at the moment the run was received,
 * so a skewed browser clock does not shift them.
 */
export const liveSeconds = (
	run: AgentWorkRun,
	nowMs: number,
	receivedAtMs?: number,
): number => {
	if (run.status !== 'running') return run.duration_seconds;
	return receivedAtMs === undefined
		? elapsedSince(Date.parse(run.started_at), nowMs)
		: run.duration_seconds + elapsedSince(receivedAtMs, nowMs);
};

export const liveTotals = (
	runs: AgentWorkRun[],
	totals: AgentWorkTotals,
	nowMs: number,
	receivedAt: Record<number, number> = {},
	totalsReceivedAtMs: number = nowMs,
): { agentSeconds: number; humanSeconds: number } => ({
	agentSeconds: runs.reduce(
		(sum, run) => sum + liveSeconds(run, nowMs, receivedAt[run.id]),
		0,
	),
	humanSeconds:
		totals.human_seconds +
		(totals.human_timer_running ? elapsedSince(totalsReceivedAtMs, nowMs) : 0),
});

export const isHttpUrl = (url: string | null): url is string =>
	!!url && /^https?:\/\/[^\s]+$/i.test(url);

export const upsertRun = (
	runs: AgentWorkRun[],
	run: AgentWorkRun,
): AgentWorkRun[] => {
	const known = runs.find((r) => r.id === run.id);
	if (!known) return [run, ...runs];
	if (known.version > run.version) return runs;
	return runs.map((r) => (r.id === run.id ? run : r));
};

/** A fetched snapshot never replaces a newer version of a run that arrived over realtime meanwhile. */
export const mergeSnapshot = (
	current: AgentWorkRun[],
	snapshot: AgentWorkRun[],
): AgentWorkRun[] => {
	const merged = snapshot.map((run) => {
		const live = current.find((r) => r.id === run.id);
		return live && live.version > run.version ? live : run;
	});
	const newer = current.filter(
		(run) => !snapshot.some((r) => r.id === run.id) && run.id > (snapshot[0]?.id ?? 0),
	);
	return [...newer, ...merged];
};
