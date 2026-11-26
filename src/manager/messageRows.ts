import { dayKey, formatDay, isGrouped } from './components/viewHelpers';
import { isMentionStateRow } from './mentions';
import type { ChatMessage } from './types';

export interface MessageRow {
	message: ChatMessage;
	day?: string;
	replyTo?: ChatMessage;
	grouped: boolean;
}

export const buildRows = (
	messages: ChatMessage[],
	now: number,
	labels: { today: string; yesterday: string },
): MessageRow[] => {
	const byId = new Map(messages.map((m) => [m.id, m]));
	const shown = messages.filter((m) => !isMentionStateRow(m));
	return shown.map((message, i) => {
		const prev = shown[i - 1];
		const newDay = !prev || dayKey(prev.ts_ms) !== dayKey(message.ts_ms);
		return {
			message,
			day: newDay ? formatDay(message.ts_ms, now, labels) : undefined,
			replyTo: message.reply_to ? byId.get(message.reply_to) : undefined,
			grouped: isGrouped(prev, message),
		};
	});
};
