export interface ReactionUser {
	id: number;
	name: string;
}

export interface ReactionSummary {
	emoji: string;
	count: number;
	reacted: boolean;
	users?: ReactionUser[];
}

export const DEFAULT_REACTION_EMOJIS = ['👍', '❤️', '😄', '🎉', '👀', '🚀'];

const cloneUsers = (users?: ReactionUser[]): ReactionUser[] =>
	(users ?? []).map((u) => ({ ...u }));

// Accept either a raw array (Java-style response) or a { data } / { reactions }
// wrapper from the reactions API before normalizing.
export const extractReactionsPayload = (body: unknown): unknown => {
	if (Array.isArray(body)) return body;
	if (body && typeof body === 'object') {
		const obj = body as Record<string, unknown>;
		return obj.data ?? obj.reactions ?? [];
	}
	return [];
};

export const normalizeReactions = (
	raw: unknown,
	currentUserId?: number,
): ReactionSummary[] => {
	if (!Array.isArray(raw)) return [];
	return raw
		.map((entry: any): ReactionSummary | null => {
			if (!entry || typeof entry.emoji !== 'string') return null;
			const users: ReactionUser[] = Array.isArray(entry.users)
				? entry.users
						.filter((u: any) => u && typeof u.id === 'number')
						.map((u: any) => ({ id: u.id, name: String(u.name ?? '') }))
				: [];
			const count =
				typeof entry.count === 'number' ? entry.count : users.length;
			const reacted =
				typeof entry.reacted === 'boolean'
					? entry.reacted
					: currentUserId != null
					? users.some((u) => u.id === currentUserId)
					: false;
			return { emoji: entry.emoji, count, reacted, users };
		})
		.filter((r): r is ReactionSummary => r !== null && r.count > 0);
};

export const mergeServerReactionForEmoji = (
	current: ReactionSummary[],
	source: ReactionSummary[],
	emoji: string,
): ReactionSummary[] => {
	const base = (Array.isArray(current) ? current : [])
		.filter((r) => r.emoji !== emoji)
		.map((r) => ({ ...r, users: cloneUsers(r.users) }));
	const incoming = (Array.isArray(source) ? source : []).find(
		(r) => r.emoji === emoji,
	);
	if (!incoming || incoming.count <= 0) {
		return base;
	}
	return [...base, { ...incoming, users: cloneUsers(incoming.users) }];
};

export const toggleReaction = (
	reactions: ReactionSummary[],
	emoji: string,
	user: ReactionUser,
): ReactionSummary[] => {
	const next = reactions.map((r) => ({ ...r, users: cloneUsers(r.users) }));
	const existing = next.find((r) => r.emoji === emoji);

	if (!existing) {
		next.push({ emoji, count: 1, reacted: true, users: [{ ...user }] });
		return next;
	}

	if (existing.reacted) {
		existing.count = Math.max(0, existing.count - 1);
		existing.reacted = false;
		existing.users = (existing.users ?? []).filter((u) => u.id !== user.id);
	} else {
		existing.count += 1;
		existing.reacted = true;
		if (!(existing.users ?? []).some((u) => u.id === user.id)) {
			existing.users = [...(existing.users ?? []), { ...user }];
		}
	}

	return next.filter((r) => r.count > 0);
};

/** Realtime payload of `comment-reactions-updated`: the comment's full reaction state. */
export interface CommentReactionsUpdatedEvent {
	comment_id: number;
	task_id: number;
	reactions: unknown;
}

/**
 * Replace one comment's reactions with the server state from a realtime event.
 * `reacted` is per viewer and absent from the broadcast, so it is derived from
 * `users` for the current user. Unknown comment ids leave the list untouched
 * (same array instance), so callers can skip a re-render.
 */
export const applyReactionsUpdate = <T extends { id: number; reactions?: ReactionSummary[] }>(
	comments: T[],
	event: CommentReactionsUpdatedEvent,
	currentUserId?: number,
): T[] => {
	const index = comments.findIndex((c) => c.id === event.comment_id);
	if (index < 0) return comments;
	const raw = Array.isArray(event.reactions)
		? event.reactions.map((r: any) => (r && typeof r === 'object' ? { ...r, reacted: undefined } : r))
		: [];
	const next = comments.slice();
	next[index] = { ...comments[index], reactions: normalizeReactions(raw, currentUserId) };
	return next;
};
