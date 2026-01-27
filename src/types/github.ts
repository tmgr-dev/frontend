export interface GithubInstallation {
  id: number;
  installation_id: number;
  account_login: string;
  account_type: 'User' | 'Organization';
  is_active: boolean;
  created_at: string;
  updated_at: string;
}

export interface GithubAvailableRepo {
  id: number;
  full_name: string;
  default_branch: string;
  private: boolean;
  installation_id: number;
}

export interface GithubRepository {
  id: number;
  github_repo_id: number;
  full_name: string;
  default_branch: string;
  is_active: boolean;
  last_synced_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface GithubCategoryStatus {
  has_installation: boolean;
  repository: GithubRepository | null;
  available_repositories: GithubAvailableRepo[];
}

export interface GithubCommit {
  id: number;
  sha: string;
  short_sha: string;
  message: string;
  author_name: string;
  author_username: string | null;
  url: string;
  committed_at: string;
  task_id: number | null;
  repository?: string;
}

export interface GithubBranch {
  id: number;
  name: string;
  head_sha: string | null;
  is_default: boolean;
  is_deleted: boolean;
  task_id: number | null;
  repository?: string;
  repository_url?: string;
}

export interface GithubPullRequest {
  id: number;
  number: number;
  title: string;
  state: 'open' | 'closed' | 'merged';
  draft: boolean;
  source_branch: string;
  target_branch: string;
  author_username: string;
  merged_by: string | null;
  merged_at: string | null;
  url: string;
  task_id: number | null;
  repository?: string;
}

export interface TaskGitActivity {
  commits: GithubCommit[];
  branches: GithubBranch[];
  pull_requests: GithubPullRequest[];
}

export interface ConnectRepoPayload {
  installation_id: number;
  github_repo_id: number;
  full_name: string;
}
