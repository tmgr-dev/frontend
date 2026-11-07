import { compileScript, parse } from '@vue/compiler-sfc';
import { readFileSync } from 'fs';
import { join } from 'path';
import ts from 'typescript';

const mounted: Array<() => Promise<void>> = [];
const unmounting: Array<() => void> = [];
const mockEnsureDefault = jest.fn(async (_ctx: unknown) => ({ uuid: 'p1' }));
const mockStore: { state: any; getters: any } = { state: {}, getters: {} };
const mockLocalContext = jest.fn(async (_w: unknown, user: unknown) => ({
	user,
}));
const mockReadPersonaCache = jest.fn(async () => null);
const mockCreateLlmChat = jest.fn(() => ({
	chat: jest.fn(),
	cancel: jest.fn(),
}));

jest.mock('vue', () => ({
	...jest.requireActual('vue'),
	onMounted: (fn: () => Promise<void>) => mounted.push(fn),
	onBeforeUnmount: (fn: () => void) => unmounting.push(fn),
}));
jest.mock('@/store', () => ({ __esModule: true, default: mockStore }));
jest.mock('@/utils/desktop', () => ({ isDesktopApp: () => true }));
jest.mock('@/local/api', () => ({ createLocalApi: () => ({}) }));
jest.mock('@/local/runtime', () => ({
	activeLocalWorkspace: () => ({ code: 'w' }),
	localContext: (...args: [unknown, unknown]) => mockLocalContext(...args),
}));
jest.mock('@/local/personas', () => ({
	ensureDefaultLocalPersona: (ctx: unknown) => mockEnsureDefault(ctx),
	listLocalPersonas: async () => [
		{
			uuid: 'p1',
			name: 'P',
			permissions: ['agent_work:write', 'comments:write'],
		},
	],
}));
jest.mock('@/local/personaCache', () => ({
	readPersonaCache: (...args: []) => mockReadPersonaCache(...args),
}));
jest.mock('@/local/llmClient', () => ({
	createLlmChat: (...args: []) => mockCreateLlmChat(...args),
}));
const mockAskPersonaOnTask = jest.fn(async (_params: any) => ({
	text: '',
	toolCalls: 0,
}));
jest.mock('@/local/personaAgent', () => ({
	askPersonaOnTask: (params: any) => mockAskPersonaOnTask(params),
	personaToolDefinition: () => ({}),
}));

const loadComponent = () => {
	const filename = join(__dirname, '../AskPersonaButton.vue');
	const { descriptor } = parse(readFileSync(filename, 'utf8'), { filename });
	const script = compileScript(descriptor, { id: 'ask-persona-button' });
	const code = ts.transpileModule(script.content, {
		compilerOptions: {
			module: ts.ModuleKind.CommonJS,
			target: ts.ScriptTarget.ES2020,
		},
	}).outputText;
	const module = { exports: {} as Record<string, any> };
	new Function('require', 'module', 'exports', code)(
		require,
		module,
		module.exports,
	);
	return module.exports.default;
};

const askAs = async (state: any, getters: any) => {
	mounted.length = 0;
	mockStore.state = state;
	mockStore.getters = getters;
	const setup = loadComponent().setup({ taskId: 1 }, { emit: jest.fn() });
	for (const fn of mounted) await fn();
	await setup.ask();
};

beforeEach(() => {
	jest.clearAllMocks();
	unmounting.length = 0;
});

test('local-only mode uses the local profile id everywhere', async () => {
	await askAs(
		{ user: {}, localProfile: { name: 'Ann' } },
		{ isLocalOnly: true },
	);
	const users = mockLocalContext.mock.calls.map((c) => c[1] as any);
	expect(users.length).toBeGreaterThan(0);
	for (const user of users)
		expect(user).toEqual({ id: -1, name: 'Ann', email: '' });
	expect(mockReadPersonaCache).toHaveBeenCalledWith('p1', -1);
	expect(mockCreateLlmChat).toHaveBeenCalledWith(expect.anything(), -1);
});

test('a signed-in user keeps the account id', async () => {
	await askAs(
		{ user: { id: 42, name: 'Bo', email: 'b@example.com' } },
		{ isLocalOnly: false },
	);
	expect(mockReadPersonaCache).toHaveBeenCalledWith('p1', 42);
	expect(mockCreateLlmChat).toHaveBeenCalledWith(expect.anything(), 42);
});

test('the account mode is read when asking, not when the window opens', async () => {
	mounted.length = 0;
	mockStore.state = {
		user: { id: 42, name: 'Bo', email: 'b@example.com' },
		localProfile: { name: 'Ann' },
	};
	mockStore.getters = { isLocalOnly: true };
	const setup = loadComponent().setup({ taskId: 1 }, { emit: jest.fn() });
	for (const fn of mounted) await fn();
	mockStore.getters = { isLocalOnly: false };
	await setup.ask();
	expect(mockReadPersonaCache).toHaveBeenCalledWith('p1', 42);
	expect(mockCreateLlmChat).toHaveBeenCalledWith(expect.anything(), 42);
});

test('shows the partial answer while asking and clears it after', async () => {
	mounted.length = 0;
	mockStore.state = { user: { id: 42, name: 'Bo', email: 'b@example.com' } };
	mockStore.getters = { isLocalOnly: false };
	const setup = loadComponent().setup({ taskId: 1 }, { emit: jest.fn() });
	for (const fn of mounted) await fn();
	const shown: string[] = [];
	mockAskPersonaOnTask.mockImplementationOnce(async (params: any) => {
		params.onText('Hel');
		shown.push(setup.partialText.value);
		params.onText('Hello');
		shown.push(setup.partialText.value);
		return { text: 'Hello', toolCalls: 0 };
	});
	await setup.ask();
	expect(shown).toEqual(['Hel', 'Hello']);
	expect(setup.partialText.value).toBe('');
});

test('a cancelled ask shows no error, a failed one does', async () => {
	mounted.length = 0;
	mockStore.state = { user: { id: 42, name: 'Bo', email: 'b@example.com' } };
	mockStore.getters = { isLocalOnly: false };
	const setup = loadComponent().setup({ taskId: 1 }, { emit: jest.fn() });
	for (const fn of mounted) await fn();
	mockAskPersonaOnTask.mockImplementationOnce(async () => {
		setup.cancelAsk();
		throw new Error('cancelled');
	});
	await setup.ask();
	expect(setup.error.value).toBeNull();
	mockAskPersonaOnTask.mockImplementationOnce(async () => {
		throw new Error('LLM server unreachable');
	});
	await setup.ask();
	expect(setup.error.value).toBe('LLM server unreachable');
});

test('a cancel pressed before the chat exists stops the ask quietly', async () => {
	mounted.length = 0;
	mockStore.state = { user: { id: 42, name: 'Bo', email: 'b@example.com' } };
	mockStore.getters = { isLocalOnly: false };
	const setup = loadComponent().setup({ taskId: 1 }, { emit: jest.fn() });
	for (const fn of mounted) await fn();
	let release!: () => void;
	mockLocalContext.mockImplementationOnce(
		(_w: unknown, user: unknown) =>
			new Promise((resolve) => {
				release = () => resolve({ user });
			}),
	);
	const asking = setup.ask();
	await Promise.resolve();
	setup.cancelAsk();
	release();
	await asking;
	expect(mockAskPersonaOnTask).not.toHaveBeenCalled();
	expect(setup.error.value).toBeNull();
	expect(setup.asking.value).toBe(false);
});

test('a failed ask keeps the streamed partial answer visible until the next ask', async () => {
	mounted.length = 0;
	mockStore.state = { user: { id: 42, name: 'Bo', email: 'b@example.com' } };
	mockStore.getters = { isLocalOnly: false };
	const setup = loadComponent().setup({ taskId: 1 }, { emit: jest.fn() });
	for (const fn of mounted) await fn();
	mockAskPersonaOnTask.mockImplementationOnce(async (params: any) => {
		params.onText('Half of the answer');
		throw new Error('stream cut off');
	});
	await setup.ask();
	expect(setup.error.value).toBe('stream cut off');
	expect(setup.partialText.value).toBe('Half of the answer');
	mockAskPersonaOnTask.mockImplementationOnce(async () => ({
		text: 'ok',
		toolCalls: 0,
	}));
	await setup.ask();
	expect(setup.error.value).toBeNull();
	expect(setup.partialText.value).toBe('');
});

test('a cancelled ask clears the streamed partial answer', async () => {
	mounted.length = 0;
	mockStore.state = { user: { id: 42, name: 'Bo', email: 'b@example.com' } };
	mockStore.getters = { isLocalOnly: false };
	const setup = loadComponent().setup({ taskId: 1 }, { emit: jest.fn() });
	for (const fn of mounted) await fn();
	mockAskPersonaOnTask.mockImplementationOnce(async (params: any) => {
		params.onText('Half of the answer');
		setup.cancelAsk();
		throw new Error('cancelled');
	});
	await setup.ask();
	expect(setup.error.value).toBeNull();
	expect(setup.partialText.value).toBe('');
});

test('the partial answer has its own scrolling block below the status row', () => {
	const filename = join(__dirname, '../AskPersonaButton.vue');
	const { descriptor } = parse(readFileSync(filename, 'utf8'), { filename });
	const template = descriptor.template!.content;
	expect(template).toMatch(
		/<div\s+v-if="partialText"[^>]*flex-col-reverse[^>]*overflow-y-auto[^>]*>\s*<div class="whitespace-pre-wrap">/,
	);
	expect(template).not.toContain('truncate');
});

test('a cancel after the stream ends emits no posted and shows no error', async () => {
	mounted.length = 0;
	mockStore.state = { user: { id: 42, name: 'Bo', email: 'b@example.com' } };
	mockStore.getters = { isLocalOnly: false };
	const emit = jest.fn();
	const setup = loadComponent().setup({ taskId: 1 }, { emit });
	for (const fn of mounted) await fn();
	mockAskPersonaOnTask.mockImplementationOnce(async () => {
		setup.cancelAsk();
		return { text: 'Hi', toolCalls: 0, posted: false };
	});
	await setup.ask();
	expect(emit).not.toHaveBeenCalled();
	expect(setup.error.value).toBeNull();
});

test('a cancel that arrives after the comment was posted still emits posted', async () => {
	mounted.length = 0;
	mockStore.state = { user: { id: 42, name: 'Bo', email: 'b@example.com' } };
	mockStore.getters = { isLocalOnly: false };
	const emit = jest.fn();
	const setup = loadComponent().setup({ taskId: 1 }, { emit });
	for (const fn of mounted) await fn();
	mockAskPersonaOnTask.mockImplementationOnce(async () => {
		setup.cancelAsk();
		return { text: 'Hi', toolCalls: 0, posted: true };
	});
	await setup.ask();
	expect(emit).toHaveBeenCalledWith('posted');
});

test('the Cancel press reaches the persona agent through isCancelled', async () => {
	mounted.length = 0;
	mockStore.state = { user: { id: 42, name: 'Bo', email: 'b@example.com' } };
	mockStore.getters = { isLocalOnly: false };
	const setup = loadComponent().setup({ taskId: 1 }, { emit: jest.fn() });
	for (const fn of mounted) await fn();
	let release!: () => void;
	let isCancelled!: () => boolean;
	mockAskPersonaOnTask.mockImplementationOnce(
		(params: any) =>
			new Promise<any>((resolve) => {
				isCancelled = params.isCancelled;
				release = () => resolve({ text: '', toolCalls: 0, posted: false });
			}),
	);
	const asking = setup.ask();
	await new Promise((resolve) => setTimeout(resolve, 0));
	expect(isCancelled()).toBe(false);
	setup.cancelAsk();
	expect(isCancelled()).toBe(true);
	release();
	await asking;
});

test('unmounting while asking cancels the ask', async () => {
	mounted.length = 0;
	mockStore.state = { user: { id: 42, name: 'Bo', email: 'b@example.com' } };
	mockStore.getters = { isLocalOnly: false };
	const cancel = jest.fn(async () => {});
	mockCreateLlmChat.mockReturnValueOnce({ chat: jest.fn(), cancel });
	const setup = loadComponent().setup({ taskId: 1 }, { emit: jest.fn() });
	for (const fn of mounted) await fn();
	let release!: () => void;
	let isCancelled!: () => boolean;
	mockAskPersonaOnTask.mockImplementationOnce(
		(params: any) =>
			new Promise<any>((resolve) => {
				isCancelled = params.isCancelled;
				release = () => resolve({ text: '', toolCalls: 0, posted: false });
			}),
	);
	const asking = setup.ask();
	await new Promise((resolve) => setTimeout(resolve, 0));
	for (const fn of unmounting) fn();
	expect(cancel).toHaveBeenCalledTimes(1);
	expect(isCancelled()).toBe(true);
	release();
	await asking;
});

test('a rejected cancel on unmount does not escape', async () => {
	mounted.length = 0;
	mockStore.state = { user: { id: 42, name: 'Bo', email: 'b@example.com' } };
	mockStore.getters = { isLocalOnly: false };
	const cancel = jest.fn(async () => {
		throw new Error('already gone');
	});
	mockCreateLlmChat.mockReturnValueOnce({ chat: jest.fn(), cancel });
	const setup = loadComponent().setup({ taskId: 1 }, { emit: jest.fn() });
	for (const fn of mounted) await fn();
	let release!: () => void;
	mockAskPersonaOnTask.mockImplementationOnce(
		() =>
			new Promise<any>((resolve) => {
				release = () => resolve({ text: '', toolCalls: 0, posted: false });
			}),
	);
	const asking = setup.ask();
	await new Promise((resolve) => setTimeout(resolve, 0));
	expect(() => unmounting.forEach((fn) => fn())).not.toThrow();
	await new Promise((resolve) => setTimeout(resolve, 0));
	release();
	await asking;
});

test('a write tool call emits posted even when the ask is cancelled', async () => {
	mounted.length = 0;
	mockStore.state = { user: { id: 42, name: 'Bo', email: 'b@example.com' } };
	mockStore.getters = { isLocalOnly: false };
	const emit = jest.fn();
	const setup = loadComponent().setup({ taskId: 1 }, { emit });
	for (const fn of mounted) await fn();
	mockAskPersonaOnTask.mockImplementationOnce(async (params: any) => {
		params.onToolCall('POST');
		setup.cancelAsk();
		throw new Error('cancelled');
	});
	await setup.ask();
	expect(emit).toHaveBeenCalledWith('posted');
});

test('read-only tool calls and a cancel emit no posted', async () => {
	mounted.length = 0;
	mockStore.state = { user: { id: 42, name: 'Bo', email: 'b@example.com' } };
	mockStore.getters = { isLocalOnly: false };
	const emit = jest.fn();
	const setup = loadComponent().setup({ taskId: 1 }, { emit });
	for (const fn of mounted) await fn();
	mockAskPersonaOnTask.mockImplementationOnce(async (params: any) => {
		params.onToolCall('GET');
		setup.cancelAsk();
		throw new Error('cancelled');
	});
	await setup.ask();
	expect(emit).not.toHaveBeenCalled();
});

test('local-only mode creates the default persona before listing', async () => {
	mounted.length = 0;
	mockStore.state = { user: {}, localProfile: { name: 'Ann' } };
	mockStore.getters = { isLocalOnly: true };
	loadComponent().setup({ taskId: 1 }, { emit: jest.fn() });
	for (const fn of mounted) await fn();
	expect(mockEnsureDefault).toHaveBeenCalledTimes(1);
});

test('a signed-in user does not create a default persona', async () => {
	mounted.length = 0;
	mockStore.state = { user: { id: 42, name: 'Bo', email: 'b@example.com' } };
	mockStore.getters = { isLocalOnly: false };
	loadComponent().setup({ taskId: 1 }, { emit: jest.fn() });
	for (const fn of mounted) await fn();
	expect(mockEnsureDefault).not.toHaveBeenCalled();
});

test('both AskPersonaButton usages in NewForm are keyed by the form id', () => {
	const source = readFileSync(
		join(__dirname, '../../../pages/NewForm.vue'),
		'utf8',
	);
	const usages = source.match(/<AskPersonaButton\b[^>]*>/g) ?? [];
	expect(usages).toHaveLength(2);
	for (const usage of usages) expect(usage).toContain(':key="form.id"');
});

test('a failing default persona creation still lists the personas', async () => {
	mounted.length = 0;
	mockStore.state = { user: {}, localProfile: { name: 'Ann' } };
	mockStore.getters = { isLocalOnly: true };
	mockEnsureDefault.mockRejectedValueOnce(new Error('db busy'));
	const setup = loadComponent().setup({ taskId: 1 }, { emit: jest.fn() });
	for (const fn of mounted) await fn();
	expect(setup.enabledPersonas.value).toHaveLength(1);
});
