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
  if (!previous) return incoming.status === 'running' ? 'agent.run_started' : null;
  if (previous.version > incoming.version) return null;
  if (previous.status === 'running' && FINISHED.includes(incoming.status)) {
    return 'agent.run_finished';
  }
  return null;
}
