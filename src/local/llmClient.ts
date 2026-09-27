import type { ChatFn, ChatMessage, ChatStreamEvent } from './personaAgent';

interface RustChatEvent {
	type: 'delta' | 'tool_call' | 'done' | 'error';
	text?: string;
	index?: number;
	id?: string | null;
	name?: string | null;
	arguments?: string;
	message?: string;
}

const invoke = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
	const core = await import('@tauri-apps/api/core');
	return core.invoke<T>(command, args);
};

/** Wraps the Rust `llm_chat`/`llm_cancel` commands (streamed via `llm://chat` events) into the
 * `ChatFn` shape `personaAgent.ts` expects. Tool-call argument chunks are streamed by index and
 * only assembled into a single tool call once the turn is `done`, since OpenAI-compatible servers
 * send `function.arguments` as text deltas. */
export const createLlmChat = (tools: unknown): { chat: ChatFn; cancel: () => Promise<void> } => {
	let activeRequestId: string | null = null;

	const chat: ChatFn = async function* (messages: ChatMessage[]): AsyncIterable<ChatStreamEvent> {
		const requestId = crypto.randomUUID();
		activeRequestId = requestId;
		const { listen } = await import('@tauri-apps/api/event');
		const queue: RustChatEvent[] = [];
		let wake: (() => void) | null = null;
		const unlisten = await listen<{ request_id: string; event: RustChatEvent }>('llm://chat', (e) => {
			if (e.payload.request_id !== requestId) return;
			queue.push(e.payload.event);
			wake?.();
		});
		try {
			await invoke('llm_chat', { requestId, messages, tools });
			const toolBuffers = new Map<number, { id: string; name: string; arguments: string }>();
			let done = false;
			while (!done) {
				if (!queue.length) {
					await new Promise<void>((resolve) => {
						wake = resolve;
					});
					wake = null;
					continue;
				}
				const event = queue.shift()!;
				if (event.type === 'delta' && event.text) {
					yield { type: 'text', delta: event.text };
				} else if (event.type === 'tool_call') {
					const index = event.index ?? 0;
					const buf = toolBuffers.get(index) ?? { id: '', name: '', arguments: '' };
					if (event.id) buf.id = event.id;
					if (event.name) buf.name = event.name;
					buf.arguments += event.arguments ?? '';
					toolBuffers.set(index, buf);
				} else if (event.type === 'done') {
					done = true;
				} else if (event.type === 'error') {
					throw new Error(event.message ?? 'LLM request failed');
				}
			}
			for (const buf of toolBuffers.values()) {
				let args: { method: string; path: string; body?: unknown };
				try {
					args = JSON.parse(buf.arguments || '{}');
				} catch {
					throw new Error(`The model sent invalid tool arguments: ${buf.arguments}`);
				}
				yield { type: 'tool_call', call: { id: buf.id, args } };
			}
		} finally {
			unlisten();
			if (activeRequestId === requestId) activeRequestId = null;
		}
	};

	const cancel = async () => {
		if (activeRequestId) await invoke('llm_cancel', { requestId: activeRequestId });
	};

	return { chat, cancel };
};
