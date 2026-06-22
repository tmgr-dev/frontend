import type { CursorAgent } from '../types/cursor';

export const ACTIVE_CURSOR_AGENT_STATUSES: ReadonlyArray<
	CursorAgent['status']
> = ['CREATING', 'RUNNING'];

export function isActiveCursorAgent(
	agent: Pick<CursorAgent, 'status'>,
): boolean {
	return ACTIVE_CURSOR_AGENT_STATUSES.includes(agent.status);
}

export function filterActiveCursorAgents(agents: CursorAgent[]): CursorAgent[] {
	if (!Array.isArray(agents)) {
		return [];
	}
	return agents.filter(isActiveCursorAgent);
}

export function cursorAgentLabel(
	agent: Pick<
		CursorAgent,
		'branch_name' | 'summary' | 'cursor_agent_id' | 'task_id'
	>,
): string {
	return (
		agent.branch_name ||
		agent.summary ||
		agent.cursor_agent_id ||
		`Task #${agent.task_id}`
	);
}

export interface CursorAgentTaskRoute {
	name: 'WorkspaceTask';
	params: { workspace_code: string; task_id: number };
}

export function cursorAgentTaskRoute(
	agent: Pick<CursorAgent, 'task_id'>,
	workspaceCode: string,
): CursorAgentTaskRoute {
	return {
		name: 'WorkspaceTask',
		params: { workspace_code: workspaceCode, task_id: agent.task_id },
	};
}
