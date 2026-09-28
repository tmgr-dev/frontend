export interface TokenInfo {
	id: string;
	prefix: string;
	personaUuid: string;
	personaName: string;
	workspaceCode: string;
	workspaceId: number;
	label: string;
	createdAt: string;
	expiresAt: string;
	lastUsedAt: string | null;
	revokedAt: string | null;
	pluginId: string | null;
}

export interface LocalAccessStatus {
	enabled: boolean;
	listening: boolean;
	socketPath: string | null;
	safeMode: boolean;
	ready: boolean;
	bridgeCommand: string;
}

const invoke = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
	const core = await import('@tauri-apps/api/core');
	return core.invoke<T>(command, args);
};

export const issueLocalToken = (args: {
	personaUuid: string;
	personaName: string;
	workspaceCode: string;
	label: string;
	expiresInDays: number;
	pluginId?: string;
}): Promise<TokenInfo> => invoke('local_token_issue', args);

export const listLocalTokens = (workspaceCode?: string): Promise<TokenInfo[]> =>
	invoke('local_token_list', { workspaceCode });

export const revokeLocalToken = (id: string): Promise<void> => invoke('local_token_revoke', { id });

export const revokeAllLocalTokens = (filter: {
	personaUuid?: string;
	pluginId?: string;
	workspaceCode?: string;
}): Promise<number> => invoke('local_token_revoke_all', filter);

export const copyLocalToken = (id: string): Promise<void> => invoke('local_token_copy', { id });

export const getLocalAccessStatus = (): Promise<LocalAccessStatus> => invoke('local_access_status');

export const setLocalAccessEnabled = (enabled: boolean): Promise<void> =>
	invoke('local_access_set_enabled', { enabled });
