export interface CursorAgentRepository {
	full_name: string;
	starting_ref: string | null;
	branch: string | null;
	pr_url: string | null;
}

export interface CursorLaunchRepository {
	full_name: string;
	default_branch: string | null;
	primary: boolean;
}

export interface CursorAgent {
	id: number;
	cursor_agent_id: string;
	task_id: number;
	user_id: number;
	project_category_id: number;
	status: 'CREATING' | 'RUNNING' | 'FINISHED' | 'STOPPED' | 'FAILED';
	branch_name: string | null;
	pr_url: string | null;
	summary: string | null;
	error_message: string | null;
	started_at: string | null;
	finished_at: string | null;
	created_at: string;
	updated_at: string;
	workspace_code?: string | null;
	api_version?: 'v0' | 'v1';
	repositories?: CursorAgentRepository[];
}

// Contract for the global GET /cursor-agents/active endpoint: workspace_code is
// REQUIRED so the indicator can jump to each agent's task in its own workspace
// (a cross-workspace list cannot assume the currently selected workspace).
export interface ActiveCursorAgent extends CursorAgent {
	workspace_code: string;
}

export interface CursorAgentMessage {
	id: string;
	type: 'user_message' | 'assistant_message';
	text: string;
}

export interface CursorConversation {
	id: string;
	messages: CursorAgentMessage[];
}

export interface CursorStatus {
	configured: boolean;
}

export interface GitHubBranch {
	name: string;
	protected: boolean;
}
