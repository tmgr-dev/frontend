import $axios from '@/plugins/axios';
import type { CursorAgent, CursorStatus, GitHubBranch } from '@/types/cursor';

export async function launchCursorAgent(taskId: number, sourceBranch?: string): Promise<CursorAgent> {
  const response = await $axios.post(`/tasks/${taskId}/cursor-agent`, {
    source_branch: sourceBranch,
  });
  return response.data.agent;
}

export async function getGitHubBranches(taskId: number): Promise<GitHubBranch[]> {
  const response = await $axios.get(`/tasks/${taskId}/cursor-agent/branches`);
  return response.data.branches;
}

export async function getCursorAgents(taskId: number): Promise<CursorAgent[]> {
  const response = await $axios.get(`/tasks/${taskId}/cursor-agent`);
  return response.data.agents;
}

export async function stopCursorAgent(taskId: number, agentId: number): Promise<void> {
  await $axios.post(`/tasks/${taskId}/cursor-agent/${agentId}/stop`);
}

export async function sendFollowUp(
  taskId: number,
  agentId: number,
  prompt: string
): Promise<void> {
  await $axios.post(`/tasks/${taskId}/cursor-agent/${agentId}/followup`, {
    prompt,
  });
}

export async function getCursorConversation(taskId: number, agentId: number): Promise<any> {
  const response = await $axios.get(`/tasks/${taskId}/cursor-agent/${agentId}/conversation`);
  return response.data;
}

export async function updateCursorApiKey(categoryId: number, apiKey: string): Promise<void> {
  await $axios.put(`/categories/${categoryId}/cursor-api-key`, {
    cursor_api_key: apiKey,
  });
}

export async function deleteCursorApiKey(categoryId: number): Promise<void> {
  await $axios.delete(`/categories/${categoryId}/cursor-api-key`);
}

export async function getCursorStatus(categoryId: number): Promise<CursorStatus> {
  const response = await $axios.get(`/categories/${categoryId}/cursor-status`);
  return response.data;
}
