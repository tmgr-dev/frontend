import { createLlmChat } from '@/local/llmClient';
import {
	isWriteTool,
	runLocalTool,
	toOpenAiTools,
} from '@/local/localAgentTools';
import type { ChatFn, ChatMessage, ToolCall } from '@/local/personaAgent';
import type { LocalContext } from '@/local/types';
import { ref } from 'vue';

export const MAX_TOOL_CALLS_PER_TURN = 8;
export const REJECTED_TEXT = 'The user rejected this action.';

export interface LocalChatEntry {
	id: number;
	role: 'user' | 'assistant' | 'tool';
	content: string;
	tool?: string;
	state?: 'done' | 'rejected';
}

export interface PendingWrite {
	key: number;
	tool: string;
	args: Record<string, unknown>;
}

export interface LocalAgentChatEnvironment {
	createChat: () => { chat: ChatFn; cancel: () => Promise<void> };
	getContext: () => Promise<{
		ctx: LocalContext;
		workspaceName: string;
	}>;
	runTool: (name: string, args: unknown, ctx: LocalContext) => Promise<string>;
}

export const localAgentUserId = async (): Promise<number> => {
	const { default: store } = await import('@/store');
	const { LOCAL_OWNER_ID } = await import('@/local/personaGate');
	return store.getters?.isLocalOnly
		? LOCAL_OWNER_ID
		: Number(store.state.user?.id) || 0;
};

const defaultEnvironment = (): LocalAgentChatEnvironment => ({
	createChat: () => {
		let inner: ReturnType<typeof createLlmChat> | null = null;
		const ensure = async () => {
			inner ??= createLlmChat(toOpenAiTools(), await localAgentUserId());
			return inner;
		};
		return {
			chat: async function* (messages) {
				yield* (await ensure()).chat(messages);
			},
			cancel: async () => {
				await (await ensure()).cancel();
			},
		};
	},
	getContext: async () => {
		const { activeLocalWorkspace, localContext } =
			await import('@/local/runtime');
		const { default: store } = await import('@/store');
		const { LOCAL_OWNER_ID } = await import('@/local/personaGate');
		const workspace = activeLocalWorkspace();
		if (!workspace) throw new Error('No local workspace is open');
		const user = store.getters?.isLocalOnly
			? {
					id: LOCAL_OWNER_ID,
					name: store.state.user?.name || store.state.localProfile?.name || '',
					email: '',
				}
			: {
					id: Number(store.state.user?.id) || 0,
					name: store.state.user?.name ?? '',
					email: store.state.user?.email ?? '',
				};
		return {
			ctx: await localContext(workspace, user),
			workspaceName: workspace.name,
		};
	},
	runTool: async (name, args, ctx) => {
		const { createLocalApi } = await import('@/local/api');
		return runLocalTool(name, args, ctx, createLocalApi(), {
			personaPrompt: async () => null,
		});
	},
});

const systemPrompt = (workspaceName: string) =>
	`You are TMGR's assistant for the local workspace "${workspaceName}". ` +
	`Use the tools to read and change tasks, comments, statuses and pages instead of guessing. ` +
	`Look ids up with a tool before using them and ask the user when something is ambiguous. ` +
	`Changes need the user's confirmation. Keep answers short.`;

export function useLocalAgentChat(
	overrides: Partial<LocalAgentChatEnvironment> = {},
) {
	const env = { ...defaultEnvironment(), ...overrides };
	const messages = ref<LocalChatEntry[]>([]);
	const busy = ref(false);
	const error = ref<string | null>(null);
	const pending = ref<PendingWrite | null>(null);

	let history: ChatMessage[] = [];
	let nextId = 1;
	let cancelCurrent: (() => Promise<void>) | null = null;
	let cancelled = false;
	let settle: ((confirmed: boolean) => void) | null = null;
	let lastUserText = '';
	let nextPendingKey = 1;

	const push = (entry: Omit<LocalChatEntry, 'id'>) => {
		messages.value.push({ id: nextId++, ...entry });
		return messages.value[messages.value.length - 1];
	};

	const askConfirmation = (call: ToolCall) =>
		new Promise<boolean>((resolve) => {
			settle = resolve;
			pending.value = {
				key: nextPendingKey++,
				tool: call.name,
				args: call.args,
			};
		});

	const resolvePending = (confirmed: boolean) => {
		const done = settle;
		settle = null;
		pending.value = null;
		done?.(confirmed);
	};

	const runTurn = async (text: string, showUser: boolean) => {
		const turnStart = history.length;
		lastUserText = text;
		if (showUser) push({ role: 'user', content: text });
		history.push({ role: 'user', content: text });
		busy.value = true;
		error.value = null;
		cancelled = false;
		let toolCalls = 0;
		try {
			const { ctx, workspaceName } = await env.getContext();
			const { chat, cancel } = env.createChat();
			cancelCurrent = cancel;
			for (;;) {
				if (cancelled) throw new Error('cancelled');
				const calls: ToolCall[] = [];
				let streamed = '';
				let entry: LocalChatEntry | null = null;
				const wire: ChatMessage[] = [
					{ role: 'system', content: systemPrompt(workspaceName) },
					...history,
				];
				for await (const event of chat(wire)) {
					if (event.type === 'text') {
						streamed += event.delta;
						entry ??= push({ role: 'assistant', content: '' });
						entry.content = streamed;
					} else if (event.type === 'tool_call') calls.push(event.call);
				}
				if (!calls.length) {
					if (!streamed.trim())
						throw new Error('The model returned an empty answer');
					history.push({ role: 'assistant', content: streamed });
					return;
				}
				if (toolCalls >= MAX_TOOL_CALLS_PER_TURN) {
					push({
						role: 'assistant',
						content: `Stopped after ${MAX_TOOL_CALLS_PER_TURN} tool calls. Ask again to continue.`,
					});
					history.push({
						role: 'assistant',
						content: streamed || 'Stopped after too many tool calls.',
					});
					return;
				}
				const batch = calls.slice(0, MAX_TOOL_CALLS_PER_TURN - toolCalls);
				history.push({
					role: 'assistant',
					content: streamed || null,
					tool_calls: batch.map((call) => ({
						id: call.id,
						type: 'function',
						function: { name: call.name, arguments: call.rawArguments },
					})),
				});
				for (const call of batch) {
					if (cancelled) throw new Error('cancelled');
					toolCalls += 1;
					let result: string;
					let state: LocalChatEntry['state'] = 'done';
					const confirmed = isWriteTool(call.name)
						? await askConfirmation(call)
						: true;
					if (cancelled) throw new Error('cancelled');
					if (confirmed) result = await env.runTool(call.name, call.args, ctx);
					else {
						result = REJECTED_TEXT;
						state = 'rejected';
					}
					push({ role: 'tool', content: '', tool: call.name, state });
					history.push({
						role: 'tool',
						tool_call_id: call.id,
						content: result,
					});
				}
			}
		} catch (e) {
			history = history.slice(0, turnStart);
			if (!cancelled) error.value = e instanceof Error ? e.message : String(e);
		} finally {
			busy.value = false;
			pending.value = null;
			settle = null;
			cancelCurrent = null;
		}
	};

	const send = async (text: string) => {
		const trimmed = text.trim();
		if (!trimmed || busy.value) return;
		await runTurn(trimmed, true);
	};

	const retry = async () => {
		if (busy.value || !error.value || !lastUserText) return;
		await runTurn(lastUserText, false);
	};

	const answer = (key: number, confirmed: boolean) => {
		if (pending.value?.key === key) resolvePending(confirmed);
	};
	const confirm = (key: number) => answer(key, true);
	const reject = (key: number) => answer(key, false);

	const cancel = async () => {
		cancelled = true;
		const stop = cancelCurrent;
		resolvePending(false);
		try {
			await stop?.();
		} catch {}
	};

	const reset = () => {
		messages.value = [];
		history = [];
		error.value = null;
	};

	return {
		messages,
		busy,
		error,
		pending,
		send,
		confirm,
		reject,
		cancel,
		retry,
		reset,
	};
}
