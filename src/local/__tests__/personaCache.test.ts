export {};

const invoke = jest.fn();

jest.mock('@tauri-apps/api/core', () => ({ invoke }));

const load = () => {
	let mod!: typeof import('../personaCache');
	jest.isolateModules(() => {
		mod = require('../personaCache');
	});
	return mod;
};

beforeEach(() => {
	invoke.mockReset();
});

it('puts a persona cache entry namespaced by user id', async () => {
	invoke.mockResolvedValue(undefined);
	const { tauriPersonaCache } = load();

	await tauriPersonaCache.put('p-1', { system_prompt: 'Hi', prompt_version: 3 }, 42);

	expect(invoke).toHaveBeenCalledWith('persona_cache_put', {
		userId: 42,
		uuid: 'p-1',
		data: { system_prompt: 'Hi', prompt_version: 3 },
	});
});

it('reads a persona cache entry namespaced by user id', async () => {
	invoke.mockResolvedValue({ system_prompt: 'Hi', prompt_version: 3 });
	const { readPersonaCache } = load();

	const result = await readPersonaCache('p-1', 42);

	expect(invoke).toHaveBeenCalledWith('persona_cache_get', { userId: 42, uuid: 'p-1' });
	expect(result).toEqual({ system_prompt: 'Hi', prompt_version: 3 });
});

it('clears both the persona cache and the persona LLM keychain entry for the given user on logout', async () => {
	invoke.mockResolvedValue(undefined);
	const { clearPersonaLlmForLogout } = load();

	await clearPersonaLlmForLogout(42);

	expect(invoke).toHaveBeenCalledWith('persona_cache_clear_for_user', { userId: 42 });
	expect(invoke).toHaveBeenCalledWith('persona_llm_clear_for_user', { userId: 42 });
});

it('clearing on logout is best effort: one failing call does not reject or stop the other', async () => {
	invoke.mockImplementation((command: string) =>
		command === 'persona_cache_clear_for_user' ? Promise.reject(new Error('locked')) : Promise.resolve(undefined),
	);
	const { clearPersonaLlmForLogout } = load();

	await expect(clearPersonaLlmForLogout(42)).resolves.toBeUndefined();
	expect(invoke).toHaveBeenCalledWith('persona_llm_clear_for_user', { userId: 42 });
});
