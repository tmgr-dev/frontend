import {
	roomKey,
	type ChatMessage,
	type MentionRef,
	type RoomInfo,
	type RoomRef,
} from './types';

export const MENTION_CAP = 'chat.mention.v1';

export const mentionSupported = (caps: readonly string[]): boolean =>
	caps.includes(MENTION_CAP);

export interface MentionItem {
	key: string;
	room: RoomRef;
	title: string;
	agent?: string;
	sessionId: string;
	handle: string;
	ok: boolean;
	reason?: string;
	risky: boolean;
	ambiguous: boolean;
}

export const mentionHandle = (title: string): string =>
	title.trim().replace(/\s+/g, '-');

export const mentionQuery = (text: string): string | null => {
	const m = /^@(\S*)$/.exec(text);
	return m ? m[1].toLowerCase() : null;
};

const sessionOf = (room: RoomRef): string => room.session ?? '';

export const mentionCandidates = (
	rooms: readonly RoomInfo[],
	activeKey: string | null,
	query: string,
): MentionItem[] => {
	const pool = rooms.filter(
		(r) =>
			r.local !== false &&
			r.mention !== undefined &&
			roomKey(r.room) !== activeKey,
	);
	const titles = new Map<string, number>();
	for (const r of pool) {
		const t = r.title.trim().toLowerCase();
		titles.set(t, (titles.get(t) ?? 0) + 1);
	}
	const items = pool
		.map((r): MentionItem => {
			const target = r.mention!;
			return {
				key: roomKey(r.room),
				room: r.room,
				title: r.title,
				agent: r.agent,
				sessionId: sessionOf(r.room),
				handle: mentionHandle(r.title),
				ok: target.ok,
				reason: target.ok ? undefined : target.reason,
				risky: target.risky === true,
				ambiguous: (titles.get(r.title.trim().toLowerCase()) ?? 0) > 1,
			};
		})
		.filter(
			(i) =>
				query === '' ||
				i.handle.toLowerCase().includes(query) ||
				i.sessionId.toLowerCase().startsWith(query),
		);
	return items.sort(
		(a, b) => Number(b.ok) - Number(a.ok) || a.title.localeCompare(b.title),
	);
};

export const firstEnabled = (items: readonly MentionItem[]): number => {
	const i = items.findIndex((x) => x.ok);
	return i < 0 ? 0 : i;
};

export const nextEnabled = (
	items: readonly MentionItem[],
	index: number,
	step: 1 | -1,
): number => {
	const n = items.length;
	for (let k = 1; k <= n; k++) {
		const i = (index + step * k + n * k) % n;
		if (items[i].ok) return i;
	}
	return index;
};

export const mentionHandleText = (item: Pick<MentionItem, 'handle'>): string =>
	`@${item.handle} `;

export const stripMention = (text: string, handle: string): string | null => {
	const lead = `@${handle}`;
	if (!text.startsWith(lead)) return null;
	const rest = text.slice(lead.length);
	if (rest !== '' && !/^\s/.test(rest)) return null;
	return rest.replace(/^\s+/, '');
};

export type MentionRole = 'question' | 'state' | 'reply';

export const mentionRole = (message: ChatMessage): MentionRole | null => {
	const m = message.mention;
	if (!m) return null;
	if (m.id === message.id) return 'question';
	if (
		(message.author.kind === 'session' || message.author.kind === 'manager') &&
		m.state === 'answered'
	)
		return 'reply';
	return 'state';
};

export const isMentionStateRow = (message: ChatMessage): boolean =>
	mentionRole(message) === 'state';

const newer = (a: ChatMessage, b: ChatMessage): boolean =>
	a.ts_ms > b.ts_ms || (a.ts_ms === b.ts_ms && a.id > b.id);

export const latestMentions = (
	messages: readonly ChatMessage[],
): Map<string, MentionRef> => {
	const best = new Map<string, ChatMessage>();
	for (const message of messages) {
		const m = message.mention;
		if (!m) continue;
		const cur = best.get(m.id);
		if (!cur || newer(message, cur)) best.set(m.id, message);
	}
	return new Map([...best].map(([id, msg]) => [id, msg.mention!]));
};

export interface InjectGate {
	show: boolean;
	refused: boolean;
	confirm: boolean;
}

export const injectGate = (
	origin: RoomInfo | undefined,
	message: ChatMessage,
): InjectGate => {
	const hidden = { show: false, refused: false, confirm: false };
	if (
		!origin ||
		origin.local === false ||
		mentionRole(message) !== 'reply' ||
		roomKey(origin.room) !== roomKey(message.room)
	)
		return hidden;
	if (origin.unrestricted) return { show: true, refused: true, confirm: false };
	return {
		show: true,
		refused: false,
		confirm: origin.room.kind === 'manager' || origin.mention?.risky === true,
	};
};
