import { i18n } from '@/i18n';
import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc';
import { renderToString } from '@vue/server-renderer';
import { readFileSync } from 'fs';
import { join } from 'path';
import ts from 'typescript';
import { createSSRApp, reactive } from 'vue';

const mockInvoke = jest.fn();
const mockStore = reactive({ state: { user: { id: 7 } as any }, getters: {} });
const mockResync = jest.fn();
const mockCollect = jest.fn();
const mockWorkspaces = jest.fn();
const mockActive = jest.fn();
const mockCreatePage = jest.fn();
const mockLlm = {
	llmComplete: jest.fn(),
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
jest.mock('vue-router', () => {
	const { h } = jest.requireActual('vue');
	return {
		RouterLink: {
			render(this: any) {
				return h('a', null, this.$slots.default?.());
			},
		},
	};
});
jest.mock('@tauri-apps/api/core', () => ({
	invoke: (...args: unknown[]) => mockInvoke(...args),
}));
jest.mock('@/actions/tmgr/workspaces', () => ({
	getWorkspaces: (...args: unknown[]) => mockWorkspaces(...args),
}));
jest.mock('@/components/ui/button', () => {
	const { h } = jest.requireActual('vue');
	return {
		Button: {
			render(this: any) {
				return h('button', this.$attrs, this.$slots.default?.());
			},
		},
	};
});
jest.mock('@/components/ui/dialog', () => {
	const { h } = jest.requireActual('vue');
	const passthrough = {
		render(this: any) {
			return h('div', null, this.$slots.default?.());
		},
	};
	return {
		Dialog: passthrough,
		DialogContent: passthrough,
		DialogFooter: passthrough,
		DialogHeader: passthrough,
		DialogTitle: passthrough,
	};
});
jest.mock('@/local/daySummaryCollect', () => ({
	collectDay: (...args: unknown[]) => mockCollect(...args),
}));
jest.mock('@/local/daySummary', () => {
	const actual = jest.requireActual('@/local/daySummary');
	return {
		...actual,
		stripThink: jest.fn((text: string) => actual.stripThink(text)),
	};
});
jest.mock('@/local/llmClient', () => ({
	llmComplete: (...args: unknown[]) => mockLlm.llmComplete(...args),
	llmLanConfirm: (...args: unknown[]) => mockLlm.llmLanConfirm(...args),
	needsConfirmationOrigin: (...args: unknown[]) =>
		mockLlm.needsConfirmationOrigin(...args),
}));
jest.mock('@/local/pages/service', () => ({
	createPage: (...args: unknown[]) => mockCreatePage(...args),
}));
jest.mock('@/local/runtime', () => ({
	activeLocalWorkspace: () => mockActive(),
	localContext: async () => ({}),
}));
jest.mock('@/local/personaGate', () => ({ LOCAL_OWNER_ID: -1 }));
jest.mock('@/utils/cloudIdentity', () => ({
	resyncCloudIdentity: (...args: unknown[]) => mockResync(...args),
}));

const loadComponent = () => {
	const filename = join(__dirname, '../DaySummaryModal.vue');
	const { descriptor } = parse(readFileSync(filename, 'utf8'), { filename });
	const script = compileScript(descriptor, { id: 'day-summary-modal' });
	const template = compileTemplate({
		source: descriptor.template!.content,
		filename,
		id: 'day-summary-modal',
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

const mountModal = async (props: Record<string, unknown> = {}) => {
	const component = loadComponent();
	const state = component.setup(props, {});
	await mockMounted!();
	const render = () =>
		renderToString(
			createSSRApp({ ...component, setup: () => state }).use(i18n),
		);
	return { state, render };
};

const task = {
	workspace: 'Home',
	key: 'TM-1',
	title: 'Write report',
	status: 'Done',
	createdToday: true,
	touchedToday: true,
	comments: 0,
	commentSnippets: [],
	moves: [],
	source: 'cloud' as const,
};

beforeEach(() => {
	mockStore.state.user = { id: 7 };
	mockStore.getters = {};
	mockResync.mockReset();
	mockResync.mockResolvedValue(undefined);
	mockInvoke.mockReset();
	mockInvoke.mockImplementation(async () => ({
		base_url: 'http://localhost:11434',
	}));
	mockCollect.mockReset();
	mockCollect.mockResolvedValue({ tasks: [task], skipped: ['Work'] });
	mockWorkspaces.mockReset();
	mockWorkspaces.mockResolvedValue([]);
	mockActive.mockReset();
	mockActive.mockReturnValue(null);
	mockCreatePage.mockReset();
	mockLlm.llmComplete.mockReset();
	mockLlm.llmLanConfirm.mockReset();
	mockLlm.needsConfirmationOrigin.mockReset();
	mockLlm.needsConfirmationOrigin.mockReturnValue(null);
});

test('without a configured AI the day falls back to the plain summary and offers setup', async () => {
	mockInvoke.mockImplementation(async () => ({ base_url: '' }));
	const { state, render } = await mountModal();

	await state.summarize();

	const html = await render();
	expect(mockLlm.llmComplete).not.toHaveBeenCalled();
	expect(html).toContain('No AI');
	expect(html).toContain('Set up local AI');
	expect(html).toContain('TM-1 Write report (now: Done)');
	expect(html).toContain('Could not read: Work');
});

test('a successful summary strips the reasoning and is labelled AI-generated', async () => {
	const { stripThink } = jest.requireMock('@/local/daySummary') as {
		stripThink: jest.Mock;
	};
	const log = jest.spyOn(console, 'log').mockImplementation(() => {});
	mockLlm.llmComplete.mockResolvedValueOnce('<think>plan</think>- Wrote TM-1');
	const { state, render } = await mountModal();

	await state.summarize();

	expect(mockLlm.llmComplete).toHaveBeenCalledWith(
		7,
		expect.objectContaining({
			system: expect.any(String),
			prompt: expect.stringContaining('TM-1'),
			maxTokens: 800,
			temperature: 0.2,
		}),
	);
	expect(stripThink).toHaveBeenCalledWith('<think>plan</think>- Wrote TM-1');
	const html = await render();
	expect(html).toContain('- Wrote TM-1');
	expect(html).toContain('AI-generated, may be inaccurate');
	expect(html).not.toContain('No AI');
	expect(log).not.toHaveBeenCalled();
	log.mockRestore();
});

test('a LAN endpoint asks before sending the titles and retries once allowed', async () => {
	const lanError = new Error('lan');
	mockLlm.needsConfirmationOrigin.mockImplementation((error: unknown) =>
		error === lanError ? 'http://192.0.2.5:11434' : null,
	);
	mockLlm.llmComplete
		.mockRejectedValueOnce(lanError)
		.mockResolvedValueOnce('- Retried summary');
	const { state, render } = await mountModal();

	await state.summarize();

	expect(state.lanOrigin.value).toBe('http://192.0.2.5:11434');
	expect(await render()).toContain(
		'Task titles, statuses and comment excerpts (also from cloud workspaces) will be sent to http://192.0.2.5:11434.',
	);
	await state.sendToLan();
	expect(mockLlm.llmLanConfirm).toHaveBeenCalledWith(
		expect.any(Number),
		'http://192.0.2.5:11434',
		true,
	);
	expect(mockLlm.llmComplete).toHaveBeenCalledTimes(2);
	expect(state.lanOrigin.value).toBeNull();
	expect(await render()).toContain('- Retried summary');
});

test('the summary shows the AI origin line only when a base URL is configured', async () => {
	const { render } = await mountModal();
	expect(await render()).toContain(
		'Sends task titles, statuses and comment excerpts to http://localhost:11434.',
	);

	mockInvoke.mockImplementation(async () => ({ base_url: '' }));
	const unconfigured = await mountModal();
	expect(await unconfigured.render()).not.toContain('Sends task titles');
});

test('the AI answer is sanitized before it is shown or saved', async () => {
	mockActive.mockReturnValue({ id: 1 });
	mockLlm.llmComplete.mockResolvedValueOnce(
		'- Wrote TM-1 ![t](https://evil.example/p?d=secret)\n- See [docs](https://example.com) <b>now</b>',
	);
	const { state, render } = await mountModal();

	await state.summarize();
	await state.savePage();

	const shown = '- Wrote TM-1 \n- See docs now';
	expect(state.summary.value).toBe(shown);
	expect(await render()).not.toContain('evil.example');
	expect(mockCreatePage).toHaveBeenCalledWith(
		{},
		expect.objectContaining({ body: shown }),
	);
});

test('an empty day gives the plain summary without calling the model', async () => {
	mockCollect.mockResolvedValueOnce({ tasks: [], skipped: [] });
	const { state, render } = await mountModal();

	await state.summarize();

	expect(mockLlm.llmComplete).not.toHaveBeenCalled();
	const html = await render();
	expect(html).toContain('Nothing recorded today.');
	expect(html).toContain('No AI');
});

test('regenerate is disabled while the LAN confirmation is open', async () => {
	const lanError = new Error('lan');
	mockLlm.needsConfirmationOrigin.mockImplementation((error: unknown) =>
		error === lanError ? 'http://192.0.2.5:11434' : null,
	);
	mockLlm.llmComplete
		.mockResolvedValueOnce('- First summary')
		.mockRejectedValueOnce(lanError);
	const { state, render } = await mountModal();
	await state.summarize();
	const regenerate = /<button[^>]*>\s*Regenerate\s*<\/button>/;
	expect((await render()).match(regenerate)?.[0]).not.toContain('disabled');

	await state.summarize();

	expect(state.lanOrigin.value).toBe('http://192.0.2.5:11434');
	expect((await render()).match(regenerate)?.[0]).toContain('disabled');
});

test('a past day is collected for that day and carries a precision note', async () => {
	const day = new Date(2026, 5, 9, 12);
	const { render } = await mountModal({ day });

	expect(mockCollect.mock.calls[0][0].now).toBe(day);
	expect(await render()).toContain(
		'Based on yesterday&#39;s records; may be less precise.',
	);
});

test('today has no precision note', async () => {
	const { render } = await mountModal();

	expect(await render()).not.toContain('may be less precise');
});
