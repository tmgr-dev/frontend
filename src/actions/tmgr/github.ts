import $axios from '@/plugins/axios';
import type {
  GithubInstallation,
  GithubAvailableRepo,
  GithubCategoryStatus,
  GithubRepository,
  ConnectRepoPayload,
  TaskGitActivity,
  GithubCommit,
  GithubBranch,
  GithubPullRequest,
} from '@/types/github';

export async function getGitHubInstallUrl(): Promise<{ url: string }> {
  const response = await $axios.get('/github/install-url');
  return response.data;
}

export async function syncGitHubInstallations(): Promise<{ message: string; count: number; installations: GithubInstallation[] }> {
  const response = await $axios.post('/github/sync');
  return response.data;
}

export async function getGitHubInstallations(): Promise<GithubInstallation[]> {
  const response = await $axios.get('/github/installations');
  return response.data;
}

export async function getGitHubRepositories(): Promise<GithubAvailableRepo[]> {
  const response = await $axios.get('/github/repositories');
  return response.data;
}

export async function removeGitHubInstallation(installationId: number): Promise<void> {
  await $axios.delete(`/github/installations/${installationId}`);
}

export async function getCategoryGitHubStatus(categoryId: number): Promise<GithubCategoryStatus> {
  const response = await $axios.get(`/categories/${categoryId}/github`);
  return response.data;
}

export async function connectCategoryRepository(
  categoryId: number,
  data: ConnectRepoPayload
): Promise<GithubRepository> {
  const response = await $axios.post(`/categories/${categoryId}/github`, data);
  return response.data;
}

export async function disconnectCategoryRepository(categoryId: number): Promise<void> {
  await $axios.delete(`/categories/${categoryId}/github`);
}

export async function syncCategoryRepository(categoryId: number): Promise<{ message: string; last_synced_at: string }> {
  const response = await $axios.post(`/categories/${categoryId}/github/sync`);
  return response.data;
}

export async function relinkCategoryRepository(categoryId: number): Promise<{ message: string; stats: { commits: number; branches: number; pull_requests: number } }> {
  const response = await $axios.post(`/categories/${categoryId}/github/relink`);
  return response.data;
}

export async function getTaskGitActivity(taskId: number): Promise<TaskGitActivity> {
  const response = await $axios.get(`/tasks/${taskId}/github`);
  return response.data;
}

export async function getTaskGitCommits(taskId: number): Promise<GithubCommit[]> {
  const response = await $axios.get(`/tasks/${taskId}/github/commits`);
  return response.data;
}

export async function getTaskGitBranches(taskId: number): Promise<GithubBranch[]> {
  const response = await $axios.get(`/tasks/${taskId}/github/branches`);
  return response.data;
}

export async function getTaskGitPullRequests(taskId: number): Promise<GithubPullRequest[]> {
  const response = await $axios.get(`/tasks/${taskId}/github/pull-requests`);
  return response.data;
}
