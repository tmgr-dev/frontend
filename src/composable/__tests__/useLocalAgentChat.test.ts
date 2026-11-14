import type {
	ChatFn,
	ChatMessage,
	ChatStreamEvent,
	ToolCall,
} from '@/local/personaAgent';
import {
	MAX_TOOL_CALLS_PER_TURN,
	useLocalAgentChat,
} from '../useLocalAgentChat';

jest.mock('@/local/llmClient', () => ({ createLlmChat: jest.fn() }));

type Script = (messages: ChatMessage[]) => ChatStreamEvent[] | Error;

const call = (name: string, id = `c-${name}`, args = {}): ToolCall => ({
	id,
	name,
	rawArguments: JSON.stringify(args),
	args,
});
const toolEvent = (c: ToolCall): ChatStreamEvent => ({
	type: 'tool_call',
	call: c,
});
const text = (delta: string): ChatStreamEvent => ({ type: 'text', delta });

const setup = (script: Script) => {
	const seen: ChatMessage[][] = [];
	const cancel = jest.fn(async () => {});
	const runTool = jest.fn(async (name: string) => `ran ${name}`);
	const chat: ChatFn = async function* (messages) {
		seen.push(JSON.parse(JSON.stringify(messages)));
		const out = script(messages);
		if (out instanceof Error) throw out;
		for (const event of out) yield event;
	};
	const api = useLocalAgentChat({
		createChat: () => ({ chat, cancel }),
		getContext: async () => ({ ctx: {} as any, workspaceName: 'Personal' }),
		runTool,
	});
	return { api, seen, cancel, runTool };
};

const last = <T>(list: T[]): T | undefined => list[list.length - 1];

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('useLocalAgentChat', () => {
	it('streams assistant text', async () => {
		const { api } = setup(() => [text('Hel'), text('lo')]);
		await api.send('hi');
		expect(api.messages.value.map((m) => [m.role, m.content])).toEqual([
			['user', 'hi'],
			['assistant', 'Hello'],
		]);
		expect(api.busy.value).toBe(false);
	});

	it('runs read tools without confirmation', async () => {
		let n = 0;
		const { api, runTool, seen } = setup(() =>
			n++ === 0 ? [toolEvent(call('get_task'))] : [text('done')],
		);
		await api.send('task?');
		expect(runTool).toHaveBeenCalledWith('get_task', {}, expect.anything());
		expect(api.pending.value).toBeNull();
		expect(last(seen[1])).toMatchObject({
			role: 'tool',
			content: 'ran get_task',
		});
		expect(seen[0][0].role).toBe('system');
	});

	it('waits for confirmation before a write tool', async () => {
		let n = 0;
		const { api, runTool } = setup(() =>
			n++ === 0
				? [toolEvent(call('create_task', 'c1', { title: 'x' }))]
				: [text('ok')],
		);
		const sending = api.send('make it');
		await tick();
		expect(api.pending.value).toEqual({
			tool: 'create_task',
			args: { title: 'x' },
		});
		expect(runTool).not.toHaveBeenCalled();
		api.confirm();
		await sending;
		expect(runTool).toHaveBeenCalledWith(
			'create_task',
			{ title: 'x' },
			expect.anything(),
		);
		expect(api.pending.value).toBeNull();
	});

	it('tells the model when the user rejects', async () => {
		let n = 0;
		const { api, runTool, seen } = setup(() =>
			n++ === 0 ? [toolEvent(call('add_comment'))] : [text('fine')],
		);
		const sending = api.send('comment');
		await tick();
		api.reject();
		await sending;
		expect(runTool).not.toHaveBeenCalled();
		expect(last(seen[1])).toMatchObject({
			role: 'tool',
			content: 'The user rejected this action.',
		});
	});

	it('cancel clears the pending confirmation and stops', async () => {
		const { api, runTool, cancel } = setup(() => [
			toolEvent(call('update_task')),
		]);
		const sending = api.send('go');
		await tick();
		expect(api.pending.value).not.toBeNull();
		await api.cancel();
		await sending;
		expect(api.pending.value).toBeNull();
		expect(cancel).toHaveBeenCalled();
		expect(runTool).not.toHaveBeenCalled();
		expect(api.busy.value).toBe(false);
		expect(api.error.value).toBeNull();
	});

	it('caps tool calls per turn', async () => {
		const { api, runTool } = setup(() => [
			toolEvent(call('get_task', `c${Math.random()}`)),
		]);
		await api.send('loop');
		expect(runTool).toHaveBeenCalledTimes(MAX_TOOL_CALLS_PER_TURN);
		expect(last(api.messages.value)?.content).toMatch(
			/Stopped after 8 tool calls/,
		);
	});

	it('surfaces chat errors and retries', async () => {
		let fail = true;
		const { api } = setup(() =>
			fail
				? new Error("Can't reach your AI server at http://localhost:11434")
				: [text('back')],
		);
		await api.send('hello');
		expect(api.error.value).toMatch(/Can't reach your AI server/);
		fail = false;
		await api.retry();
		expect(api.error.value).toBeNull();
		expect(api.messages.value.filter((m) => m.role === 'user')).toHaveLength(1);
		expect(last(api.messages.value)?.content).toBe('back');
	});
});
