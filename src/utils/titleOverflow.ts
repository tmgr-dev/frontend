export const TITLE_LIMIT = 255;

export interface TitleSplit {
	insert: string;
	overflow: string;
}

const isHighSurrogate = (code: number) => code >= 0xd800 && code <= 0xdbff;
const isLowSurrogate = (code: number) => code >= 0xdc00 && code <= 0xdfff;

// `before` / `after` are the title parts left of / right of the selection that
// the inserted text replaces, so `after` stays in the title.
export function splitTitleInsert(
	before: string,
	after: string,
	text: string,
	limit = TITLE_LIMIT,
): TitleSplit {
	const room = limit - before.length - after.length;
	if (text.length <= room) return { insert: text, overflow: '' };
	if (room <= 0) return { insert: '', overflow: text };

	// The first overflow char may itself be the whitespace that ends the last
	// fitting word, so a word that fits exactly is not moved.
	const window = text.slice(0, room + 1);
	const boundary = window.search(/\s\S*$/);
	if (boundary >= 0) {
		return {
			insert: text.slice(0, boundary).trimEnd(),
			overflow: text.slice(boundary + 1),
		};
	}

	let cut = room;
	if (
		isHighSurrogate(text.charCodeAt(cut - 1)) &&
		isLowSurrogate(text.charCodeAt(cut))
	)
		cut -= 1;
	return { insert: text.slice(0, cut), overflow: text.slice(cut) };
}

export function carriedSeparator(existing: string): string {
	return existing.trim() ? '\n\n' : '';
}

export function joinCarriedMarkdown(carried: string, existing: string): string {
	return carried + carriedSeparator(existing) + existing;
}

export function escapeCarriedHtml(text: string): string {
	return text
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
		.replace(/\n/g, '<br>');
}

// Editor.js description_json is a blocks document or a legacy HTML string.
export function prependCarriedBlock(
	existing: unknown,
	carried: string,
): unknown {
	const html = escapeCarriedHtml(carried);
	if (typeof existing === 'string')
		return existing.trim() ? `<p>${html}</p>${existing}` : `<p>${html}</p>`;
	const blocks = (existing as { blocks?: unknown } | null)?.blocks;
	return {
		...(existing as object | null),
		blocks: [
			{ type: 'paragraph', data: { text: html } },
			...(Array.isArray(blocks) ? blocks : []),
		],
	};
}
