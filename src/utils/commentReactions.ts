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
