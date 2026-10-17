export interface TrackedRun {
	id: number;
	status: string;
	version: number;
}

export type AgentRunEvent = 'agent.run_started' | 'agent.run_finished';

const FINISHED = ['succeeded', 'failed', 'cancelled'];

export function agentRunEvent(
	known: TrackedRun[],
	incoming: TrackedRun,
	loaded: boolean,
): AgentRunEvent | null {
	if (!loaded) return null;
	const previous = known.find((run) => run.id === incoming.id);
	if (!previous) return 'agent.run_started';
	if (previous.version > incoming.version) return null;
	if (previous.status === 'running' && FINISHED.includes(incoming.status)) {
		return 'agent.run_finished';
	}
	return null;
}

export const AGENT_WORK_ANCHOR = 'task-form.agent-work';

export function showAgentWorkEmptyState(o: {
	loaded: boolean;
	runs: unknown[];
	tourAnchor?: string | null;
}): boolean {
	return o.loaded && o.runs.length === 0 && o.tourAnchor === AGENT_WORK_ANCHOR;
}
