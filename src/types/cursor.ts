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
