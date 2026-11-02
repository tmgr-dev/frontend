import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc';
import { renderToString } from '@vue/server-renderer';
import { readFileSync } from 'fs';
import { join } from 'path';
import ts from 'typescript';
import { createSSRApp } from 'vue';

const mockInvoke = jest.fn();
const mockStore = { state: { user: { id: 7 } }, getters: {} };

jest.mock('vue', () => ({
	...jest.requireActual('vue'),
	onMounted: () => undefined,
}));
jest.mock('vuex', () => ({ useStore: () => mockStore }));
jest.mock('@tauri-apps/api/core', () => ({
	invoke: (...args: unknown[]) => mockInvoke(...args),
}));
jest.mock('@/actions/tmgr/personas', () => ({ listPersonas: jest.fn() }));
jest.mock('@/components/local/LocalPersonaConnectDialog.vue', () => ({
	__esModule: true,
	default: { render: () => null },
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
jest.mock('@/components/ui/switch', () => ({ Switch: { render: () => null } }));
jest.mock('@/local/localTokens', () => ({}));
jest.mock('@/local/personaCache', () => ({ tauriPersonaCache: {} }));
jest.mock('@/local/personaGate', () => ({
	LOCAL_OWNER_ID: -1,
	PERSONA_PERMISSIONS: [],
}));
jest.mock('@/local/personas', () => ({}));
jest.mock('@/local/runtime', () => ({
	activeLocalWorkspace: () => ({ code: 'w' }),
	localContext: jest.fn(),
}));
jest.mock('@/pluginSystem/state', () => ({
	localAccessConnect: { current: null },
	resolveLocalAccessConnect: jest.fn(),
}));
jest.mock('@/utils/desktop', () => ({ supportsLocalAccess: () => false }));

const REMOTE_HTTP_ERROR =
	'Refusing to save an API key for a remote endpoint over plain HTTP; use HTTPS.';

const loadComponent = () => {
	const filename = join(__dirname, '../LocalPersonasPanel.vue');
	const { descriptor } = parse(readFileSync(filename, 'utf8'), { filename });
	const script = compileScript(descriptor, { id: 'local-personas-panel' });
	const template = compileTemplate({
		source: descriptor.template!.content,
		filename,
		id: 'local-personas-panel',
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

const mountPanel = async () => {
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
	mockInvoke.mockReset();
	mockInvoke.mockImplementation(async (command: string) => {
		if (command === 'llm_config_get') return config;
	});
});

test('a rejected save shows the plain-string error and keeps the typed key', async () => {
	const { state, render } = await mountPanel();
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
	const { state, render } = await mountPanel();
	mockInvoke.mockRejectedValueOnce(REMOTE_HTTP_ERROR);

	await state.removeApiKey();

	expect(await render()).toContain(REMOTE_HTTP_ERROR);
	expect(state.savingLlm.value).toBe(false);
});

test('an Error rejection shows its message', async () => {
	const { state, render } = await mountPanel();
	mockInvoke.mockRejectedValueOnce(new Error('boom'));

	await state.saveLlmConfig();

	expect(await render()).toContain('boom');
});

test('a successful retry clears the previous error', async () => {
	const { state, render } = await mountPanel();
	state.llmForm.apiKey = 'sk-typed';
	mockInvoke.mockRejectedValueOnce(REMOTE_HTTP_ERROR);
	await state.saveLlmConfig();
	expect(await render()).toContain(REMOTE_HTTP_ERROR);

	await state.saveLlmConfig();

	expect(await render()).not.toContain(REMOTE_HTTP_ERROR);
	expect(state.llmForm.apiKey).toBe('');
});
