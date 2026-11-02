import { compileScript, parse } from '@vue/compiler-sfc';
import { readFileSync } from 'fs';
import { join } from 'path';
import ts from 'typescript';

const mounted: Array<() => Promise<void>> = [];
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
}));
jest.mock('@/store', () => ({ __esModule: true, default: mockStore }));
jest.mock('@/utils/desktop', () => ({ isDesktopApp: () => true }));
jest.mock('@/local/api', () => ({ createLocalApi: () => ({}) }));
jest.mock('@/local/runtime', () => ({
	activeLocalWorkspace: () => ({ code: 'w' }),
	localContext: (...args: [unknown, unknown]) => mockLocalContext(...args),
}));
jest.mock('@/local/personas', () => ({
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
