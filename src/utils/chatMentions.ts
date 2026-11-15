export interface MentionQuery {
	start: number;
	query: string;
}

export const findMentionQuery = (
	text: string,
	caret: number,
): MentionQuery | null => {
	const match = /(^|\s)@([^\s@]*)$/.exec(text.slice(0, caret));
	if (!match) return null;
	return { start: caret - match[2].length - 1, query: match[2] };
};

export const insertMention = (
	text: string,
	caret: number,
	mention: MentionQuery,
	name: string,
): { text: string; caret: number } => {
	const inserted = `@${name} `;
	return {
		text: text.slice(0, mention.start) + inserted + text.slice(caret),
		caret: mention.start + inserted.length,
	};
};
