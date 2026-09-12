interface CommentLike {
	created_at?: string | null;
}

const at = (comment: CommentLike): number => {
	const time = new Date(comment?.created_at || 0).getTime();
	return Number.isNaN(time) ? 0 : time;
};

/**
 * Comments read as a conversation: oldest first, so the newest one sits next to
 * the composer instead of at the far end of the thread. Sorting a copy keeps the
 * loaded list in the order the API returned it, and equal timestamps keep their
 * arrival order (Array.sort is stable), which matters when several comments land
 * in the same second over the socket.
 */
export const sortCommentsOldestFirst = <T extends CommentLike>(comments: T[]): T[] =>
	[...(comments || [])].sort((a, b) => at(a) - at(b));
