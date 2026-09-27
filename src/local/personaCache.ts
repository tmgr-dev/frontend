import type { PersonaCache } from './personas';

const invoke = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
	const core = await import('@tauri-apps/api/core');
	return core.invoke<T>(command, args);
};

/** `persona_cache_put/get` (Rust): JSON under the app data dir, keyed by persona uuid, outside any
 * workspace folder — see design 4.2 and src-tauri/src/persona_cache.rs. */
export const tauriPersonaCache: PersonaCache = {
	put: (uuid, data) => invoke('persona_cache_put', { uuid, data }),
};

export const readPersonaCache = (uuid: string): Promise<{ system_prompt?: string | null; prompt_version?: number | null } | null> =>
	invoke('persona_cache_get', { uuid });
