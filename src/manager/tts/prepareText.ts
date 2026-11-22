const CODE = '';
const MAX_CHUNK = 300;
const PHRASE = { ru: 'блок кода пропущен', en: 'code block omitted' } as const;

export type SpeechLang = keyof typeof PHRASE;

export interface SpeechText {
	lang: SpeechLang;
	chunks: string[];
}

const detectLang = (text: string): SpeechLang => {
	const letters = text.match(/\p{L}/gu)?.length ?? 0;
	if (!letters) return 'en';
	const cyrillic = text.match(/\p{Script=Cyrillic}/gu)?.length ?? 0;
	return cyrillic / letters >= 0.3 ? 'ru' : 'en';
};

const stripMarkdown = (md: string): string =>
	md
		.replace(/\r\n?/g, '\n')
		.replace(
			/^[ \t]*(`{3,}|~{3,})[^\n]*\n[\s\S]*?(?:^[ \t]*\1[ \t]*$|(?![\s\S]))/gm,
			`\n${CODE}\n`,
		)
		.replace(/`([^`\n]+)`/g, '$1')
		.replace(/!\[[^\]]*\]\([^)]*\)/g, '')
		.replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
		.replace(/\[([^\]]+)\]\[[^\]]*\]/g, '$1')
		.replace(/^\s*\[[^\]]+\]:\s*\S+.*$/gm, '')
		.replace(/<(https?:\/\/[^>]+)>/g, '')
		.replace(/https?:\/\/\S+|www\.\S+/g, '')
		.replace(/<\/?[a-zA-Z][^>]*>/g, '')
		.replace(
			/^[ \t]*\|?[ \t]*:?-{3,}:?[ \t]*(\|[ \t]*:?-{3,}:?[ \t]*)*\|?[ \t]*$/gm,
			'',
		)
		.replace(/^[ \t]*\|(.*)\|[ \t]*$/gm, (_, row: string) =>
			row
				.split('|')
				.map((c) => c.trim())
				.filter(Boolean)
				.join(', '),
		)
		.replace(/^\s{0,3}#{1,6}\s+/gm, '')
		.replace(/^\s*>\s?/gm, '')
		.replace(/^\s*([-*_])(\s*\1){2,}\s*$/gm, '')
		.replace(/^[ \t]*(?:[-*+]|\d+[.)])[ \t]+/gm, '')
		.replace(/(\*\*|__)(.+?)\1/g, '$2')
		.replace(/~~(.+?)~~/g, '$1')
		.replace(/\*(\S(?:[^*\n]*\S)?)\*/g, '$1')
		.replace(/(^|\W)_(\S(?:[^_\n]*\S)?)_(?=\W|$)/g, '$1$2')
		.replace(/\p{Extended_Pictographic}|\p{Emoji_Presentation}|️|‍/gu, '');

const splitLong = (text: string, max: number): string[] => {
	const out: string[] = [];
	let rest = text;
	while (rest.length > max) {
		let cut = rest.lastIndexOf(' ', max);
		if (cut <= 0) cut = max;
		out.push(rest.slice(0, cut).trim());
		rest = rest.slice(cut).trim();
	}
	if (rest) out.push(rest);
	return out;
};

export const chunkText = (text: string, max = MAX_CHUNK): string[] => {
	const pieces = (text.match(/[^.!?…]+(?:[.!?…]+|$)/g) ?? [])
		.map((s) => s.trim())
		.filter(Boolean)
		.flatMap((s) => (s.length > max ? splitLong(s, max) : [s]));
	const chunks: string[] = [];
	let current = '';
	for (const piece of pieces) {
		if (current && current.length + 1 + piece.length <= max) {
			current += ` ${piece}`;
		} else {
			if (current) chunks.push(current);
			current = piece;
		}
	}
	if (current) chunks.push(current);
	return chunks;
};

export const prepareSpeech = (md: string): SpeechText => {
	const stripped = stripMarkdown(md);
	const lang = detectLang(stripped);
	const spoken = stripped
		.replace(new RegExp(CODE, 'g'), `${PHRASE[lang]}.`)
		.replace(/\s+/g, ' ')
		.trim();
	return { lang, chunks: spoken ? chunkText(spoken) : [] };
};
