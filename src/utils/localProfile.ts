export interface LocalProfile {
	id: -1;
	uuid: string;
	name: string;
	created_at: string;
}

const invoke = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
	const core = await import('@tauri-apps/api/core');
	return core.invoke<T>(command, args);
};

export const loadLocalProfile = (): Promise<LocalProfile | null> =>
	invoke<LocalProfile | null>('local_profile_get');

export const createLocalProfile = (name?: string): Promise<LocalProfile> =>
	invoke<LocalProfile>('local_profile_create', { name: name || null });

export const updateLocalProfile = (name: string): Promise<LocalProfile> =>
	invoke<LocalProfile>('local_profile_update', { name });
