import type { PersonaCache } from './personas';

const invoke = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
	const core = await import('@tauri-apps/api/core');
	return core.invoke<T>(command, args);
};

/** Rust-side JSON under the app data dir, outside the workspace folder, so it never rides along when the folder is copied or moved. */
export const tauriPersonaCache: PersonaCache = {
	put: (uuid, data) => invoke('persona_cache_put', { uuid, data }),
};

export const readPersonaCache = (uuid: string): Promise<{ system_prompt?: string | null; prompt_version?: number | null } | null> =>
	invoke('persona_cache_get', { uuid });
