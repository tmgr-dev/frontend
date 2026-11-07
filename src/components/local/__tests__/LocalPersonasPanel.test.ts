import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc';
import { renderToString } from '@vue/server-renderer';
import { readFileSync } from 'fs';
import { join } from 'path';
import ts from 'typescript';
import { createSSRApp, reactive } from 'vue';

const mockInvoke = jest.fn();
const mockStore = reactive({ state: { user: { id: 7 } as any }, getters: {} });
const mockResync = jest.fn();
const mockPersonas = {
	ensureDefaultLocalPersona: jest.fn(),
	listLocalPersonas: jest.fn(),
};
const mockListTokens = jest.fn();
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
jest.mock('@/local/localTokens', () => ({
	listLocalTokens: (...args: unknown[]) => mockListTokens(...args),
}));
jest.mock('@/local/personaCache', () => ({ tauriPersonaCache: {} }));
jest.mock('@/local/personaGate', () => ({
	LOCAL_OWNER_ID: -1,
	PERSONA_PERMISSIONS: [],
}));
jest.mock('@/local/personas', () => mockPersonas);
jest.mock('@/components/local/LocalAiSettings.vue', () => {
	const { h } = jest.requireActual('vue');
	return {
		__esModule: true,
		default: { render: () => h('section', { 'data-llm-settings': '' }) },
	};
});
jest.mock('@/local/runtime', () => ({
	activeLocalWorkspace: () => ({ code: 'w' }),
	localContext: async () => ({}),
	onActiveLocalWorkspaceChange: () => () => undefined,
}));
jest.mock('@/utils/cloudIdentity', () => ({
	resyncCloudIdentity: (...args: unknown[]) => mockResync(...args),
}));
jest.mock('@/pluginSystem/state', () => ({
	localAccessConnect: { current: null },
	resolveLocalAccessConnect: jest.fn(),
}));
jest.mock('@/utils/desktop', () => ({ supportsLocalAccess: () => false }));

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

const mountLive = () => {
	const component = loadComponent();
	const state = component.setup({}, {});
	const render = () =>
		renderToString(createSSRApp({ ...component, setup: () => state }));
	return { state, render };
};

test('the lists still load when ensuring the default agent fails', async () => {
	(mockStore.getters as any).isLocalOnly = true;
	const error = jest.spyOn(console, 'error').mockImplementation(() => {});
	mockPersonas.ensureDefaultLocalPersona.mockRejectedValueOnce(
		new Error('database is locked'),
	);
	mockPersonas.listLocalPersonas.mockResolvedValueOnce([
		{ uuid: 'p-1', name: 'Local agent', permissions: [] },
	]);
	mockListTokens.mockResolvedValueOnce([]);
	try {
		const { state } = mountLive();
		await mockMounted!();
		expect(state.personas.value.map((p: any) => p.uuid)).toEqual(['p-1']);
		expect(mockListTokens).toHaveBeenCalled();
		expect(error).toHaveBeenCalled();
	} finally {
		delete (mockStore.getters as any).isLocalOnly;
		error.mockRestore();
	}
});

const renderPanel = async (props: Record<string, unknown> = {}) => {
	const component = loadComponent();
	const state = component.setup(props, {});
	return await renderToString(
		createSSRApp({ ...component, setup: () => state }, props),
	);
};

test('the panel renders the AI settings by default', async () => {
	(mockStore.getters as any).isLocalOnly = true;
	try {
		expect(await renderPanel()).toContain('data-llm-settings');
	} finally {
		delete (mockStore.getters as any).isLocalOnly;
	}
});

test('the panel leaves out the AI settings when the page already shows them', async () => {
	(mockStore.getters as any).isLocalOnly = true;
	try {
		expect(await renderPanel({ showLlmSettings: false })).not.toContain(
			'data-llm-settings',
		);
	} finally {
		delete (mockStore.getters as any).isLocalOnly;
	}
});
