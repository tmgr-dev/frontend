import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc';
import { renderToString } from '@vue/server-renderer';
import { readFileSync } from 'fs';
import { join } from 'path';
import ts from 'typescript';
import { createSSRApp, nextTick, reactive, ref } from 'vue';

import { i18n } from '@/i18n';

const mockInvoke = jest.fn();
const mockStore = reactive({
	state: { aiPanelOpen: true },
	getters: {},
	commit: jest.fn(),
});

jest.mock('@/store', () => ({ __esModule: true, default: mockStore }));
jest.mock('vue-router', () => ({ useRouter: () => ({ push: jest.fn() }) }));
jest.mock('@tauri-apps/api/core', () => ({
	invoke: (...args: unknown[]) => mockInvoke(...args),
}));
jest.mock('@/utils/cloudIdentity', () => ({
	resyncCloudIdentity: async () => {},
}));
jest.mock('@/composable/useLocalAgentChat', () => ({
	localAgentUserId: async () => -1,
	useLocalAgentChat: () => ({
		messages: ref([]),
		busy: ref(false),
		error: ref(null),
		pending: ref(null),
		send: jest.fn(),
		confirm: jest.fn(),
		reject: jest.fn(),
		cancel: jest.fn(),
		retry: jest.fn(),
		reset: jest.fn(),
	}),
}));

const loadComponent = () => {
	const filename = join(__dirname, '../LocalAgentPanel.vue');
	const { descriptor } = parse(readFileSync(filename, 'utf8'), { filename });
	const script = compileScript(descriptor, { id: 'local-agent-panel' });
	const template = compileTemplate({
		source: descriptor.template!.content,
		filename,
		id: 'local-agent-panel',
		compilerOptions: { bindingMetadata: script.bindings },
	});
	const run = (code: string) => {
		const module = { exports: {} as Record<string, any> };
		new Function(
			'require',
			'module',
			'exports',
			ts.transpileModule(code, {
				compilerOptions: {
					module: ts.ModuleKind.CommonJS,
					target: ts.ScriptTarget.ES2020,
				},
			}).outputText,
		)(require, module, module.exports);
		return module.exports;
	};
	return {
		...run(script.content).default,
		render: run(template.code).render,
	};
};

const renderPanel = async (config: unknown) => {
	mockInvoke.mockReset();
	mockInvoke.mockResolvedValue(config);
	const component = loadComponent();
	let state: any;
	const make = () =>
		createSSRApp({
			...component,
			setup: (props: any, ctx: any) => {
				state ??= component.setup(props, ctx);
				return state;
			},
		}).use(i18n);
	await renderToString(make());
	for (let i = 0; i < 5; i++) {
		await nextTick();
		await new Promise((resolve) => setTimeout(resolve, 0));
	}
	return renderToString(make());
};

const REMOTE =
	'Your AI server is not on this computer or local network. Workspace data you send in this chat leaves this device.';

const base = {
	base_url: 'http://localhost:11434',
	model: 'llama3.1',
	has_api_key: false,
};

test('shows the privacy banner for a remote server', async () => {
	const html = await renderPanel({ ...base, is_local: false });
	expect(html).toContain('remote-banner');
	expect(html).toContain(REMOTE);
	expect(html).not.toContain('no-server');
});

test('hides the privacy banner for a local server', async () => {
	const html = await renderPanel({ ...base, is_local: true });
	expect(html).not.toContain('remote-banner');
	expect(html).not.toContain(REMOTE);
	expect(mockInvoke).toHaveBeenCalledWith('llm_config_get', { userId: -1 });
});

test('shows the empty state without a base URL', async () => {
	const html = await renderPanel({ ...base, base_url: '', is_local: true });
	expect(html).toContain('no-server');
	expect(html).toContain('Open AI settings');
});
