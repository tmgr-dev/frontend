import { api } from './client';

export async function getWorkspaces(): Promise<any[]> {
	const { data } = await api.get('workspaces');
	return data?.data ?? [];
}

export async function getWorkspaceStatuses(): Promise<any[]> {
	const { data } = await api.get('workspaces/statuses');
	return data?.data ?? [];
}

export async function getWorkspaceMembers(workspaceId: number): Promise<any[]> {
	const { data } = await api.get(`workspaces/${workspaceId}/members`);
	return data?.data ?? [];
}

export async function getUserFeatureToggles(): Promise<Record<string, any>> {
	const { data } = await api.get('user/feature-toggles');
	return data?.data ?? {};
}

export async function getWorkspaceFeatureToggles(
	workspaceId: number,
): Promise<Record<string, any>> {
	const { data } = await api.get(`workspaces/${workspaceId}/feature-toggles`);
	return data?.data ?? {};
}
