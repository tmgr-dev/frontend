import type { CursorAgent } from '../types/cursor';
import { isActiveCursorAgent } from './cursorAgents';

type FetchConversation = (taskId: number, agentId: number) => Promise<unknown>;

export function createCursorConversationSync(
	taskId: number,
	fetchConversation: FetchConversation,
) {
	const seen = new Map<number, CursorAgent['status']>();
	const inFlight = new Map<number, Promise<boolean>>();

	const syncOne = (agentId: number): Promise<boolean> => {
		const running = inFlight.get(agentId);
		if (running) return running;
		const request = fetchConversation(taskId, agentId)
			.then(() => true)
			.catch((e) => {
				console.error('Failed to sync Cursor conversation:', e);
				return false;
			})
			.finally(() => inFlight.delete(agentId));
		inFlight.set(agentId, request);
		return request;
	};

	async function update(agents: CursorAgent[]): Promise<boolean> {
		const due: number[] = [];
		for (const agent of agents) {
			const previous = seen.get(agent.id);
			seen.set(agent.id, agent.status);
			const active = isActiveCursorAgent(agent);
			if (
				previous === undefined
					? agent.status !== 'CREATING'
					: !active && isActiveCursorAgent({ status: previous })
			) {
				due.push(agent.id);
			}
		}
		const results = await Promise.all(due.map(syncOne));
		return results.some(Boolean);
	}

	return { update };
}
