import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc';
import { renderToString } from '@vue/server-renderer';
import { readFileSync } from 'fs';
import { join } from 'path';
import ts from 'typescript';
import { createSSRApp, nextTick, reactive } from 'vue';

const mockInvoke = jest.fn();
const mockStore = reactive({ state: { user: { id: 7 } as any }, getters: {} });
const mockResync = jest.fn();
const mockLlm = {
	llmDetect: jest.fn(),
	llmModels: jest.fn(),
	llmTest: jest.fn(),
	llmLanConfirm: jest.fn(),
	needsConfirmationOrigin: jest.fn(),
};
let mockMounted: (() => Promise<void>) | undefined;

jest.mock('vue', () => ({
	...jest.requireActual('vue'),
	onMounted: (fn: () => Promise<void>) => {
		mockMounted = fn;
	},
}));
jest.mock('vuex', () => ({ useStore: () => mockStore }));
jest.mock('@tauri-apps/api/core', () => ({
	invoke: (...args: unknown[]) => mockInvoke(...args),
}));
jest.mock('@/components/ui/button', () => {
	const { h } = jest.requireActual('vue');
	return {
		Button: {
			render(this: any) {
				return h('button', null, this.$slots.default?.());
			},
		},
	};
});
jest.mock('@/components/ui/input', () => {
	const { h } = jest.requireActual('vue');
	return { Input: { render: () => h('input') } };
});
jest.mock('@/local/personaGate', () => ({ LOCAL_OWNER_ID: -1 }));
jest.mock('@/local/llmClient', () => ({
	llmDetect: (...args: unknown[]) => mockLlm.llmDetect(...args),
	llmModels: (...args: unknown[]) => mockLlm.llmModels(...args),
	llmTest: (...args: unknown[]) => mockLlm.llmTest(...args),
	llmLanConfirm: (...args: unknown[]) => mockLlm.llmLanConfirm(...args),
	needsConfirmationOrigin: (...args: unknown[]) =>
		mockLlm.needsConfirmationOrigin(...args),
}));
jest.mock('@/utils/cloudIdentity', () => ({
	resyncCloudIdentity: (...args: unknown[]) => mockResync(...args),
}));

const loadComponent = () => {
	const filename = join(__dirname, '../LocalAiSettings.vue');
	const { descriptor } = parse(readFileSync(filename, 'utf8'), { filename });
	const script = compileScript(descriptor, { id: 'local-ai-settings' });
	const template = compileTemplate({
		source: descriptor.template!.content,
		filename,
		id: 'local-ai-settings',
		compilerOptions: { bindingMetadata: script.bindings },
	});
	const transpile = (code: string) =>
		ts.transpileModule(code, {
			compilerOptions: {
				module: ts.ModuleKind.CommonJS,
				target: ts.ScriptTarget.ES2020,
			},
		}).outputText;
	const run = (code: string) => {
		const module = { exports: {} as Record<string, any> };
		new Function('require', 'module', 'exports', transpile(code))(
			require,
			module,
			module.exports,
		);
		return module.exports;
	};
	return {
		...run(script.content).default,
		render: run(template.code).render,
	};
};

const mountSettings = async () => {
	const component = loadComponent();
	let state: any;
	const render = async () =>
		renderToString(
			createSSRApp({
				...component,
				setup: (props: any, ctx: any) => {
					state ??= component.setup(props, ctx);
					return state;
				},
			}),
		);
	await render();
	return { state, render };
};

const config = {
	base_url: 'http://localhost:11434',
	model: 'llama3.1',
	has_api_key: true,
	is_local: true,
};

beforeEach(() => {
	mockStore.state.user = { id: 7 };
	mockLlm.needsConfirmationOrigin.mockReset();
	mockLlm.needsConfirmationOrigin.mockReturnValue(null);
	mockLlm.llmDetect.mockReset();
	mockLlm.llmModels.mockReset();
	mockLlm.llmTest.mockReset();
	mockLlm.llmLanConfirm.mockReset();
	mockResync.mockReset();
	mockResync.mockResolvedValue(undefined);
	mockInvoke.mockReset();
	mockInvoke.mockImplementation(async (command: string) => {
		if (command === 'llm_config_get') return config;
	});
});

const REMOTE_HTTP_ERROR =
	'Refusing to save an API key for a remote endpoint over plain HTTP; use HTTPS.';

test('a rejected save shows the plain-string error and keeps the typed key', async () => {
	const { state, render } = await mountSettings();
	state.llmForm.baseUrl = 'http://example.com';
	state.llmForm.apiKey = 'sk-typed';
	mockInvoke.mockRejectedValueOnce(REMOTE_HTTP_ERROR);

	await state.saveLlmConfig();

	expect(await render()).toContain(REMOTE_HTTP_ERROR);
	expect(state.savingLlm.value).toBe(false);
	expect(state.llmForm.apiKey).toBe('sk-typed');
	expect(state.llmForm.baseUrl).toBe('http://example.com');
});

test('a rejected key removal shows the error', async () => {
	const { state, render } = await mountSettings();
	mockInvoke.mockRejectedValueOnce(REMOTE_HTTP_ERROR);

	await state.removeApiKey();

	expect(await render()).toContain(REMOTE_HTTP_ERROR);
	expect(state.savingLlm.value).toBe(false);
});

test('an Error rejection shows its message', async () => {
	const { state, render } = await mountSettings();
	mockInvoke.mockRejectedValueOnce(new Error('boom'));

	await state.saveLlmConfig();

	expect(await render()).toContain('boom');
});

test('a successful retry clears the previous error', async () => {
	const { state, render } = await mountSettings();
	state.llmForm.apiKey = 'sk-typed';
	mockInvoke.mockRejectedValueOnce(REMOTE_HTTP_ERROR);
	await state.saveLlmConfig();
	expect(await render()).toContain(REMOTE_HTTP_ERROR);

	await state.saveLlmConfig();

	expect(await render()).not.toContain(REMOTE_HTTP_ERROR);
	expect(state.llmForm.apiKey).toBe('');
});

const mountLive = () => {
	const component = loadComponent();
	const state = component.setup({}, {});
	const render = () =>
		renderToString(createSSRApp({ ...component, setup: () => state }));
	return { state, render };
};

const settle = async () => {
	for (let i = 0; i < 5; i++) {
		await nextTick();
		await new Promise((resolve) => setTimeout(resolve, 0));
	}
};

test('resync is awaited before llm_config_get', async () => {
	const { state } = mountLive();
	let release!: () => void;
	mockResync.mockReturnValueOnce(new Promise<void>((r) => (release = r)));
	const pending = state.refreshLlmConfig();
	await settle();
	expect(mockInvoke).not.toHaveBeenCalled();
	release();
	await pending;
	expect(mockInvoke).toHaveBeenCalledWith('llm_config_get', { userId: 7 });
});

test('resync is awaited before llm_config_set', async () => {
	const { state } = mountLive();
	let release!: () => void;
	mockResync.mockReturnValueOnce(new Promise<void>((r) => (release = r)));
	const pending = state.saveLlmConfig();
	await settle();
	expect(mockInvoke).not.toHaveBeenCalled();
	release();
	await pending;
	expect(mockInvoke.mock.calls[0][0]).toBe('llm_config_set');
});

test('the account arriving later refreshes only the key status and slot state', async () => {
	mockStore.state.user = { id: 0 };
	mockInvoke.mockImplementation(async (command: string) => {
		if (command === 'llm_config_get')
			return { ...config, base_url: 'http://saved', model: 'saved' };
	});
	const { state } = mountLive();
	state.llmForm.baseUrl = 'http://typed';
	state.llmForm.model = 'typed-model';
	state.llmForm.apiKey = 'sk-typed';
	mockStore.state.user = { id: 9 };
	await settle();
	expect(mockInvoke).toHaveBeenCalledWith('llm_config_get', { userId: 9 });
	expect(state.llmStatus.hasApiKey).toBe(true);
	expect(state.llmForm.baseUrl).toBe('http://typed');
	expect(state.llmForm.model).toBe('typed-model');
	expect(state.llmForm.apiKey).toBe('sk-typed');
});

test('a status response for a previous account is dropped', async () => {
	mockStore.state.user = { id: 0 };
	let answer!: (value: unknown) => void;
	mockInvoke.mockImplementation(
		(command: string) =>
			command === 'llm_config_get' &&
			new Promise((resolve) => (answer = resolve)),
	);
	const { state } = mountLive();
	mockStore.state.user = { id: 9 };
	await settle();
	mockStore.state.user = { id: 10 };
	answer({ ...config, has_api_key: true, slot_state: 'ok' });
	await settle();
	expect(state.llmStatus.hasApiKey).toBe(false);
});

test('a not ready slot shows the note', async () => {
	mockInvoke.mockImplementation(async (command: string) => {
		if (command === 'llm_config_get')
			return { ...config, has_api_key: false, slot_state: 'not_ready' };
	});
	const { state, render } = mountLive();
	await state.refreshLlmConfig();
	expect(await render()).toContain(
		'Your account is not available to the desktop app yet. Reload the app or sign in again to manage the API key.',
	);
});

test('another account slot shows its note and a ready slot shows none', async () => {
	mockInvoke.mockImplementation(async (command: string) => {
		if (command === 'llm_config_get')
			return { ...config, slot_state: 'other_account' };
	});
	const { state, render } = mountLive();
	await state.refreshLlmConfig();
	expect(await render()).toContain(
		'These AI provider settings belong to another account.',
	);
	mockInvoke.mockImplementation(async () => config);
	await state.refreshLlmConfig();
	const html = await render();
	expect(html).not.toContain('belong to another account');
	expect(html).not.toContain('not available to the desktop app');
});

test('a saved key tied to another server asks to be entered again', async () => {
	mockInvoke.mockImplementation(async (command: string) => {
		if (command === 'llm_config_get')
			return { ...config, key_needs_reentry: true };
	});
	const { state, render } = mountLive();
	await state.refreshLlmConfig();
	const html = await render();
	expect(html).toContain(
		'Enter your API key again to use it with this server.',
	);
	expect(html).not.toContain('leave blank to keep it');
	mockInvoke.mockImplementation(async () => config);
	await state.refreshLlmConfig();
	const after = await render();
	expect(after).not.toContain('Enter your API key again');
	expect(after).toContain('leave blank to keep it');
});

test('the saved config loads on mount for the local owner', async () => {
	(mockStore.getters as any).isLocalOnly = true;
	try {
		const { state } = mountLive();
		await mockMounted!();
		expect(mockInvoke).toHaveBeenCalledWith('llm_config_get', { userId: -1 });
		expect(state.llmForm.baseUrl).toBe(config.base_url);
	} finally {
		delete (mockStore.getters as any).isLocalOnly;
	}
});

test('detect fills the base URL without saving and reports the models found', async () => {
	const { state, render } = mountLive();
	await state.refreshLlmConfig();
	mockLlm.llmDetect.mockResolvedValueOnce([
		{ base_url: 'http://127.0.0.1:1234', models: 3 },
	]);

	await state.detectLocal();

	expect(state.llmForm.baseUrl).toBe('http://127.0.0.1:1234');
	expect(mockInvoke).not.toHaveBeenCalledWith(
		'llm_config_set',
		expect.anything(),
	);
	expect(await render()).toContain('Found 3 models at http://127.0.0.1:1234');
});

test('detect with no local server says so', async () => {
	const { state, render } = mountLive();
	mockLlm.llmDetect.mockResolvedValueOnce([]);

	await state.detectLocal();

	expect(await render()).toContain(
		'No local server found on the default Ollama / LM Studio ports.',
	);
});

test('refresh models fills the suggestions and a failure leaves them empty', async () => {
	const { state, render } = mountLive();
	await state.refreshLlmConfig();
	mockLlm.llmModels.mockResolvedValueOnce(['llama3.1', 'qwen2.5']);

	await state.refreshModels();

	expect(mockLlm.llmModels).toHaveBeenCalledWith(7);
	expect(await render()).toContain('value="qwen2.5"');
	mockLlm.llmModels.mockRejectedValueOnce(new Error('404 Not Found'));

	await state.refreshModels();

	const html = await render();
	expect(html).toContain('404 Not Found');
	expect(html).not.toContain('value="qwen2.5"');
});

test('test connection shows the verdict and is blocked while the form differs from the saved config', async () => {
	const { state, render } = mountLive();
	await state.refreshLlmConfig();
	mockLlm.llmTest.mockResolvedValueOnce('OK: llama3.1 answered');

	await state.testConnection();

	expect(await render()).toContain('OK: llama3.1 answered');
	state.llmForm.model = 'other-model';
	expect(state.llmDirty.value).toBe(true);
	expect(await render()).toContain(
		'Save first - these use the saved settings.',
	);
	state.llmForm.model = 'llama3.1';
	expect(state.llmDirty.value).toBe(false);
	expect(await render()).not.toContain('Save first');
});

test('a LAN server asks for confirmation and retries the action once allowed', async () => {
	const { state, render } = mountLive();
	await state.refreshLlmConfig();
	const lanError = new Error('lan');
	mockLlm.needsConfirmationOrigin.mockImplementation((error: unknown) =>
		error === lanError ? 'http://192.0.2.5:11434' : null,
	);
	mockLlm.llmTest
		.mockRejectedValueOnce(lanError)
		.mockResolvedValueOnce('OK: retried');

	await state.testConnection();

	expect(state.lanPrompt.value?.origin).toBe('http://192.0.2.5:11434');
	expect(await render()).toContain(
		'Task titles, statuses and comment excerpts (also from cloud workspaces) will be sent to http://192.0.2.5:11434.',
	);
	await state.allowLan();
	expect(mockLlm.llmLanConfirm).toHaveBeenCalledWith(
		'http://192.0.2.5:11434',
		true,
	);
	expect(mockLlm.llmTest).toHaveBeenCalledTimes(2);
	expect(state.lanPrompt.value).toBeNull();
	expect(await render()).toContain('OK: retried');
});

test('forget confirmation is offered for a non-loopback saved URL and revokes it', async () => {
	mockInvoke.mockImplementation(async (command: string) => {
		if (command === 'llm_config_get')
			return { ...config, base_url: 'http://192.0.2.5:11434' };
	});
	const { state, render } = mountLive();
	await state.refreshLlmConfig();
	expect(await render()).toContain('Forget confirmation');

	await state.forgetLan();

	expect(mockLlm.llmLanConfirm).toHaveBeenCalledWith(
		'http://192.0.2.5:11434',
		false,
	);
});

test('forget confirmation is hidden for a localhost saved URL', async () => {
	const { state, render } = mountLive();
	await state.refreshLlmConfig();
	expect(await render()).not.toContain('Forget confirmation');
	expect(await render()).toContain('OLLAMA_CONTEXT_LENGTH');
});

test('forget confirmation is offered for a saved URL that differs from the confirmed origin only by the default port', async () => {
	mockInvoke.mockImplementation(async (command: string) => {
		if (command === 'llm_config_get')
			return { ...config, base_url: 'http://nas.lan:80' };
	});
	const { state, render } = mountLive();
	await state.refreshLlmConfig();
	expect(await render()).toContain('Forget confirmation');

	await state.forgetLan();

	expect(mockLlm.llmLanConfirm).toHaveBeenCalledWith('http://nas.lan', false);
});

test('forget confirmation appears after allowing a loopback URL saved with an explicit default port', async () => {
	mockInvoke.mockImplementation(async (command: string) => {
		if (command === 'llm_config_get')
			return { ...config, base_url: 'http://127.0.0.1:80' };
	});
	const { state, render } = mountLive();
	await state.refreshLlmConfig();
	expect(await render()).not.toContain('Forget confirmation');
	const lanError = new Error('lan');
	mockLlm.needsConfirmationOrigin.mockImplementation((error: unknown) =>
		error === lanError ? 'http://127.0.0.1:80' : null,
	);
	mockLlm.llmTest
		.mockRejectedValueOnce(lanError)
		.mockResolvedValueOnce('OK: retried');
	await state.testConnection();

	await state.allowLan();

	expect(await render()).toContain('Forget confirmation');
});
