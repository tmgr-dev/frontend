// STUB contract: mirrors the envelope planned by track A (TM-1018). Replace with A's generated types when they land.

export type ChatKind = 'room' | 'session' | 'machine';

export type SessionStatus = 'working' | 'waiting' | 'failed' | 'idle' | 'done';

export interface Chat {
	id: string;
	kind: ChatKind;
	title: string;
	agent?: string;
	machine?: string;
	ctx_tokens?: number;
	status?: SessionStatus;
	unread: number;
	pinned: boolean;
	last_preview?: string;
	last_ts?: number;
}

export type AuthorKind = 'owner' | 'manager' | 'session' | 'agent';

export interface Author {
	kind: AuthorKind;
	id: string;
	label: string;
}

export type MessageKind = 'text' | 'decision' | 'system';

export interface Attachment {
	id: string;
	name: string;
	mime: string;
	size: number;
}

export interface ManagerMessage {
	id: string;
	chat_id: string;
	author: Author;
	ts: number;
	body_md: string;
	attachments: Attachment[];
	reply_to?: string;
	kind: MessageKind;
	decision_id?: string;
}

export interface Draft {
	body_md: string;
	reply_to?: string;
	attachments: Attachment[];
}

export interface SlashCommand {
	name: string;
	description: string;
	args?: string;
	group: string;
	dangerous: boolean;
}

export interface SearchQuery {
	text: string;
	chat_id?: string;
}

export interface SearchHit {
	chat_id: string;
	message_id: string;
	ts: number;
	snippet: string;
}
