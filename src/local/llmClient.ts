import { resyncCloudIdentity } from '@/utils/cloudIdentity';
import type { ChatFn, ChatMessage, ChatStreamEvent } from './personaAgent';

const LENGTH_CUT_OFF = 'The answer was cut off: the model hit its length limit';

interface RustChatEvent {
	type: 'delta' | 'tool_call' | 'done' | 'error';
	text?: string;
	index?: number;
	id?: string | null;
	name?: string | null;
	arguments?: string;
	message?: string;
}

const invoke = async <T>(
	command: string,
	args?: Record<string, unknown>,
): Promise<T> => {
	const core = await import('@tauri-apps/api/core');
	return core.invoke<T>(command, args);
};

const NEEDS_CONFIRMATION = 'needs_confirmation:';

export const llmDetect = () =>
	invoke<{ base_url: string; models: number }[]>('llm_detect');

export const llmModels = async (userId: number): Promise<string[]> => {
	await resyncCloudIdentity();
	return invoke<string[]>('llm_models', { userId });
};

export const llmTest = async (userId: number): Promise<string> => {
	await resyncCloudIdentity();
	return invoke<string>('llm_test', { userId });
};

export const llmComplete = async (
	userId: number,
	options: {
		system: string;
		prompt: string;
		maxTokens: number;
		temperature: number;
	},
): Promise<string> => {
	await resyncCloudIdentity();
	return invoke<string>('llm_complete', { userId, ...options });
};

export const llmLanConfirm = async (
	userId: number,
	origin: string,
	confirmed: boolean,
): Promise<void> => {
	await resyncCloudIdentity();
	return invoke<void>('llm_lan_confirm', { origin, confirmed, userId });
};

export const needsConfirmationOrigin = (error: unknown): string | null => {
	const message =
		error instanceof Error
			? error.message
			: typeof error === 'string'
			? error
			: null;
	if (!message?.startsWith(NEEDS_CONFIRMATION)) return null;
	return message.slice(NEEDS_CONFIRMATION.length).trim() || null;
};

/** Tool-call argument chunks stream by index; only assembled into a call once the turn is `done`. */
export interface LlmChatOptions {
	maxTokens?: number;
	deadlineAt?: number;
}

export const createLlmChat = (
	tools: unknown,
	userId: number,
	options: LlmChatOptions = {},
): { chat: ChatFn; cancel: () => Promise<void> } => {
	let activeRequestId: string | null = null;
	let cancelled = false;

	const chat: ChatFn = async function* (
		messages: ChatMessage[],
	): AsyncIterable<ChatStreamEvent> {
		if (cancelled) throw new Error('cancelled');
		const requestId = crypto.randomUUID();
		activeRequestId = requestId;
		const { listen } = await import('@tauri-apps/api/event');
		const { getCurrentWebviewWindow } = await import(
			'@tauri-apps/api/webviewWindow'
		);
		const queue: RustChatEvent[] = [];
		let wake: (() => void) | null = null;
		const unlisten = await listen<{ request_id: string; event: RustChatEvent }>(
			'llm://chat',
			(e) => {
				if (e.payload.request_id !== requestId) return;
				queue.push(e.payload.event);
				wake?.();
			},
			{ target: getCurrentWebviewWindow().label },
		);
		try {
			await resyncCloudIdentity();
			if (cancelled) throw new Error('cancelled');
			const totalTimeoutMs =
				options.deadlineAt === undefined
					? undefined
					: options.deadlineAt - Date.now();
			if (totalTimeoutMs !== undefined && totalTimeoutMs <= 0)
				throw new Error(
					'The answer took longer than 15 minutes and was stopped.',
				);
			await invoke('llm_chat', {
				userId,
				requestId,
				messages,
				tools,
				maxTokens: options.maxTokens,
				totalTimeoutMs,
			});
			if (cancelled) await invoke('llm_cancel', { requestId });
			type ToolBuffer = { id: string; name: string; arguments: string };
			const toolBuffers: ToolBuffer[] = [];
			const currentByIndex = new Map<number, ToolBuffer>();
			let done = false;
			let sawText = false;
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
					sawText = true;
					yield { type: 'text', delta: event.text };
				} else if (event.type === 'tool_call') {
					const index = event.index ?? 0;
					let buf = currentByIndex.get(index);
					if (!buf || (event.id && buf.id && buf.id !== event.id)) {
						buf = { id: '', name: '', arguments: '' };
						currentByIndex.set(index, buf);
						toolBuffers.push(buf);
					}
					if (event.id) buf.id = event.id;
					if (event.name) buf.name = event.name;
					buf.arguments += event.arguments ?? '';
				} else if (event.type === 'done') {
					done = true;
				} else if (
					event.type === 'error' &&
					sawText &&
					toolBuffers.length === 0 &&
					event.message?.startsWith(LENGTH_CUT_OFF)
				) {
					yield { type: 'truncated' };
					done = true;
				} else if (event.type === 'error') {
					throw new Error(event.message ?? 'LLM request failed');
				}
			}
			for (const [position, buf] of toolBuffers.entries()) {
				const rawArguments = buf.arguments || '{}';
				let args: { method: string; path: string; body?: unknown };
				try {
					args = JSON.parse(rawArguments);
				} catch {
					const shown =
						rawArguments.length > 200
							? `${rawArguments.slice(0, 200)}…`
							: rawArguments;
					throw new Error(`The model sent invalid tool arguments: ${shown}`);
				}
				yield {
					type: 'tool_call',
					call: {
						id: buf.id || `call_${requestId}_${position}`,
						name: buf.name,
						rawArguments,
						args,
					},
				};
			}
		} finally {
			unlisten();
			if (activeRequestId === requestId) activeRequestId = null;
		}
	};

	const cancel = async () => {
		cancelled = true;
		if (activeRequestId)
			await invoke('llm_cancel', { requestId: activeRequestId });
	};

	return { chat, cancel };
};
