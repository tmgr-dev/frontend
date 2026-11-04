export type CardKind =
	| 'decision'
	| 'review'
	| 'release'
	| 'check'
	| 'question'
	| 'permission'
	| 'stopped';

export type ReplyVia = 'mcp' | 'cli' | 'hook';

export type CardState =
	'open' | 'deferred' | 'answered' | 'closed' | 'dismissed';

export type FactState = 'ok' | 'bad' | 'wait';

export type CheckState = 'ok' | 'bad' | 'todo';

export interface CardSource {
	agent: string;
	session?: string;
	cwd?: string;
	title?: string;
	pid?: number;
	runner_session?: string;
}

export interface CardFact {
	state: FactState;
	text: string;
	link?: string;
}

export interface CardOption {
	id: string;
	label: string;
	recommended: boolean;
}

export interface CardCheck {
	state: CheckState;
	text: string;
}

export interface CardDiffRef {
	repo: string;
	base?: string;
	head?: string;
}

export type CardLinkKind = 'pr' | 'commit' | 'tag' | 'task' | 'url';

export interface CardLinkItem {
	kind: CardLinkKind;
	url: string;
	label?: string;
}

export interface CardPatchInfo {
	label: string;
	bytes: number;
}

export interface CardLinks {
	task?: string;
	pr?: string;
	items?: CardLinkItem[];
}

export interface CardMessage {
	seq: number;
	author: 'agent' | 'owner';
	text: string;
	file?: string;
	line?: number;
	action?: MessageAction;
	created_ms: number;
	delivered?: boolean;
}

export interface LineComment {
	file: string;
	line: number;
	text: string;
}

export interface Answer {
	choice?: string;
	note?: string;
	free_text?: string;
	line_comments: LineComment[];
	deferred: boolean;
	dismissed: boolean;
}

export type MessageAction = 'reply' | 'fix';

export interface OutgoingMessage {
	text: string;
	file?: string;
	line?: number;
	action: MessageAction;
}

export interface CardView {
	id: string;
	created_ms: number;
	state: CardState;
	session_gone: boolean;
	closed_reason?: string;
	can_reply: boolean;
	listening: boolean;
	answer?: Answer;
	messages: CardMessage[];
	source: CardSource;
	kind: CardKind;
	blocking: boolean;
	title: string;
	context?: string;
	question?: string;
	facts: CardFact[];
	options: CardOption[];
	diff?: CardDiffRef;
	diffs: CardDiffRef[];
	patches: CardPatchInfo[];
	updated_ms?: number;
	checks: CardCheck[];
	links: CardLinks;
	reply_via: ReplyVia;
}

export type DiffLineKind = 'add' | 'del' | 'ctx';

export interface DiffLine {
	kind: DiffLineKind;
	old?: number;
	new?: number;
	text: string;
}

export interface DiffHunk {
	header: string;
	lines: DiffLine[];
}

export interface DiffFile {
	path: string;
	added: number;
	removed: number;
	hunks: DiffHunk[];
}

export interface DiffView {
	files: DiffFile[];
	truncated: boolean;
	error?: string;
}

export interface DiffGroup {
	key: string;
	label: string;
	kind: 'repo' | 'patch';
	base?: string;
	head?: string;
	view: DiffView;
}

export interface DiffGroups {
	groups: DiffGroup[];
}
