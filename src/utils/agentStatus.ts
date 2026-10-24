export type AgentSessionState =
	'working' | 'waiting' | 'blocked' | 'idle' | 'done' | 'exited';

export interface AgentSession {
	session_id: string;
	workspace_id: number;
	task: { id: number; key: string | null } | null;
	state: AgentSessionState;
	state_since: string | null;
	started_at: string | null;
	updated_at: string | null;
	ended_at: string | null;
	agent_profile: string | null;
	host_label: string | null;
	user: { id: number; name: string } | null;
	stale: boolean;
}

export interface TaskAgentStatus {
	state: AgentSessionState;
	count: number;
	stale: boolean;
	sessions: AgentSession[];
}

const PRIORITY: AgentSessionState[] = [
	'blocked',
	'working',
	'waiting',
	'idle',
	'done',
	'exited',
];

const rank = (state: AgentSessionState): number => {
	const index = PRIORITY.indexOf(state);
	return index === -1 ? PRIORITY.length : index;
};

export const STATE_LABELS: Record<AgentSessionState, string> = {
	working: 'Working',
	waiting: 'Waiting',
	blocked: 'Blocked',
	idle: 'Idle',
	done: 'Done',
	exited: 'Exited',
};

export const STATE_GLYPHS: Record<AgentSessionState, string> = {
	working: '●',
	waiting: '◆',
	blocked: '◆',
	idle: '○',
	done: '✓',
	exited: '○',
};

export const upsertSession = (
	sessions: AgentSession[],
	incoming: AgentSession,
): AgentSession[] => {
	const rest = sessions.filter((s) => s.session_id !== incoming.session_id);
	return incoming.ended_at ? rest : [...rest, incoming];
};

export const replaceSessions = (snapshot: AgentSession[]): AgentSession[] =>
	snapshot.reduce<AgentSession[]>(
		(acc, session) => upsertSession(acc, session),
		[],
	);

export const bySessionPriority = (a: AgentSession, b: AgentSession): number =>
	rank(a.state) - rank(b.state) ||
	Number(a.stale) - Number(b.stale) ||
	(Date.parse(b.updated_at ?? '') || 0) - (Date.parse(a.updated_at ?? '') || 0);

export const groupByTask = (
	sessions: AgentSession[],
): Record<number, TaskAgentStatus> => {
	const grouped: Record<number, AgentSession[]> = {};
	sessions.forEach((session) => {
		const taskId = session.task?.id;
		if (taskId == null) return;
		(grouped[taskId] ??= []).push(session);
	});
	return Object.fromEntries(
		Object.entries(grouped).map(([taskId, list]) => {
			const sorted = [...list].sort(bySessionPriority);
			return [
				taskId,
				{
					state: sorted[0].state,
					count: sorted.length,
					stale: sorted[0].stale,
					sessions: sorted,
				},
			];
		}),
	);
};

export const formatSince = (iso: string | null, nowMs: number): string => {
	const at = Date.parse(iso ?? '');
	if (Number.isNaN(at)) return '';
	const minutes = Math.max(0, Math.floor((nowMs - at) / 60000));
	if (minutes < 1) return 'just now';
	if (minutes < 60) return `${minutes} min ago`;
	const hours = Math.floor(minutes / 60);
	if (hours < 24) return `${hours} h ago`;
	return `${Math.floor(hours / 24)} d ago`;
};

export const sessionTooltip = (
	session: AgentSession,
	nowMs: number,
): string => {
	const who = session.agent_profile || 'agent';
	const where = session.host_label ? ` on ${session.host_label}` : '';
	const since = formatSince(session.state_since, nowMs);
	const parts = [
		`${STATE_LABELS[session.state] ?? session.state}: ${who}${where}`,
	];
	if (since) parts.push(`since ${since}`);
	if (session.stale) parts.push('no recent updates');
	return parts.join(' · ');
};

export const statusTooltip = (status: TaskAgentStatus, nowMs: number): string =>
	status.sessions.map((session) => sessionTooltip(session, nowMs)).join('\n');
