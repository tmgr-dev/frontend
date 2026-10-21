import type {
	AgentWorkDeploy,
	AgentWorkRelease,
	AgentWorkRun,
	AgentWorkTotals,
} from '@/actions/tmgr/agentWork';

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

const REPO_SEGMENT = /^[A-Za-z0-9_.-]+$/;
const HEX_SHA = /^[0-9a-f]{7,64}$/i;

export const commitUrl = (
	repo: string | null | undefined,
	sha: string | null | undefined,
): string | null => {
	if (!repo || !sha || !HEX_SHA.test(sha)) return null;
	const parts = repo.split('/');
	if (
		parts.length !== 2 ||
		parts.some((part) => !REPO_SEGMENT.test(part) || /^\.+$/.test(part))
	)
		return null;
	return `https://github.com/${repo}/commit/${sha}`;
};

export interface CommitRow {
	sha: string;
	short: string;
	message: string | null;
	url: string | null;
}

export interface CommitGroup {
	key: string;
	header: string;
	commits: CommitRow[];
}

export const commitGroups = (runs: AgentWorkRun[]): CommitGroup[] => {
	const groups = new Map<string, CommitGroup>();
	for (const run of runs) {
		for (const commit of run.commits ?? []) {
			const repo = commit.repo || null;
			const branch = commit.branch || run.branch || null;
			const key = `${repo ?? ''}\u0000${branch ?? ''}`;
			const group = groups.get(key) ?? {
				key,
				header: [repo, branch].filter(Boolean).join(' · '),
				commits: [],
			};
			groups.set(key, group);
			if (group.commits.some((known) => known.sha === commit.sha)) continue;
			group.commits.push({
				sha: commit.sha,
				short: (commit.sha || '').slice(0, 7),
				message: commit.message,
				url: commitUrl(repo, commit.sha),
			});
		}
	}
	return [...groups.values()];
};

export const formatRelease = (
	release: AgentWorkRelease,
): {
	label: string;
	ciUrl: string | null;
	digest: string | null;
	digestTitle: string | null;
} => ({
	label: [release.repo, release.tag].filter(Boolean).join(' '),
	ciUrl: isHttpUrl(release.ci_url) ? release.ci_url : null,
	digest: release.digest ? release.digest.slice(0, 12) : null,
	digestTitle: release.digest || null,
});

export const formatDeploy = (
	deploy: AgentWorkDeploy,
): { label: string; url: string | null } => ({
	label: `${deploy.env} · ${deploy.status}`,
	url: isHttpUrl(deploy.url) ? deploy.url : null,
});

export interface TimeRow {
	key: 'active' | 'wall' | 'waiting' | 'agents';
	label: string;
	seconds: number;
}

export const timeRows = (
	agentSeconds: number,
	agents: {
		activeSeconds?: number | null;
		wallClockSeconds?: number | null;
		waitingSeconds?: number | null;
	} | null,
): TimeRow[] => {
	const active = agents?.activeSeconds ?? null;
	const wall = agents?.wallClockSeconds ?? null;
	const waiting = agents?.waitingSeconds ?? null;
	const rows: TimeRow[] = [];
	if (active === null) {
		rows.push({ key: 'agents', label: 'Agents', seconds: agentSeconds });
	} else {
		rows.push({ key: 'active', label: 'Agents active', seconds: active });
	}
	if (wall !== null)
		rows.push({ key: 'wall', label: 'Wall-clock', seconds: wall });
	if (waiting !== null)
		rows.push({
			key: 'waiting',
			label: 'Waiting between runs',
			seconds: waiting,
		});
	if (active !== null && Math.abs(agentSeconds - active) >= 1)
		rows.push({
			key: 'agents',
			label: 'Agent time (sum of runs)',
			seconds: agentSeconds,
		});
	return rows;
};

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
		(run) =>
			!snapshot.some((r) => r.id === run.id) && run.id > (snapshot[0]?.id ?? 0),
	);
	return [...newer, ...merged];
};
