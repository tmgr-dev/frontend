import type { PersonaCache } from './personas';

const invoke = async <T>(
	command: string,
	args?: Record<string, unknown>,
): Promise<T> => {
	const core = await import('@tauri-apps/api/core');
	return core.invoke<T>(command, args);
};

/** Rust-side JSON under the app data dir, outside the workspace folder, so it never rides along when the folder is copied or moved. */
export const tauriPersonaCache: PersonaCache = {
	put: (uuid, data, userId) =>
		invoke('persona_cache_put', { userId, uuid, data }),
};

export const readPersonaCache = (
	uuid: string,
	userId: number,
): Promise<{
	system_prompt?: string | null;
	prompt_version?: number | null;
} | null> => invoke('persona_cache_get', { userId, uuid });

/** Called on logout so the next account signed into this machine starts clean. Both run; any failure rejects. */
export const clearPersonaLlmForLogout = async (
	userId: number,
): Promise<void> => {
	const commands = [
		'persona_cache_clear_for_user',
		'persona_llm_clear_for_user',
	];
	const results = await Promise.allSettled(
		commands.map((command) => invoke(command, { userId })),
	);
	const failed = results.flatMap((result, i) =>
		result.status === 'rejected'
			? [`${commands[i]}: ${result.reason?.message ?? result.reason}`]
			: [],
	);
	if (failed.length) {
		throw new Error(failed.join('; '));
	}
};
