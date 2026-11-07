import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc';
import { renderToString } from '@vue/server-renderer';
import { readFileSync } from 'fs';
import { join } from 'path';
import ts from 'typescript';
import { createSSRApp, nextTick, reactive } from 'vue';

const mockInvoke = jest.fn();
const mockStore = reactive({ state: { user: { id: 7 } as any }, getters: {} });
const mockResync = jest.fn();
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
