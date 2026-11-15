import type {
	ChatMember,
	ChatMessage,
	PersonaReply,
} from '@/actions/tmgr/chats';

const REFUSAL_REASONS = [
	'busy',
	'rate_limited',
	'not_allowed',
	'unavailable',
	'daily_budget',
	'personal_budget',
	'budget_unavailable',
];

export const personaRefusalKey = (reason: string | undefined): string =>
	`chats.persona.refusal.${
		reason && REFUSAL_REASONS.includes(reason) ? reason : 'unavailable'
	}`;

export const isPersonaMessage = (message: ChatMessage): boolean =>
	message.author.kind === 'persona';

export const canStopMessage = (
	message: ChatMessage,
	selfId: number | null,
): boolean =>
	isPersonaMessage(message) &&
	message.status === 'pending' &&
	!message.deleted &&
	selfId !== null &&
	message.invoked_by?.id === selfId;

export const personaMembers = (members: ChatMember[]): ChatMember[] =>
	members.filter((m) => m.kind === 'persona');

export const refusedReplies = (
	replies: PersonaReply[] | undefined,
): PersonaReply[] => (replies ?? []).filter((r) => r.status === 'refused');

export const queuedReplyIds = (replies: PersonaReply[] | undefined): number[] =>
	(replies ?? [])
		.filter((r) => r.status === 'queued' && r.message_id != null)
		.map((r) => r.message_id as number);
