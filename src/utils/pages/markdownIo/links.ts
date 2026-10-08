import { parseTmgrUrl } from '@/utils/pages/tmgrLinks';
import { encodePath, relativePath } from './paths';

export type LinkKind = 'link' | 'image' | 'wikilink' | 'embed' | 'definition';

export interface LinkToken {
	kind: LinkKind;
	label: string;
	target: string;
	text: string;
	build: (target: string, label?: string) => string;
}

export type LinkRewriter = (token: LinkToken) => string | null | undefined;

const FENCE_OPEN = /^ {0,3}(`{3,}|~{3,})/;
const INLINE_CODE = /(?<!`)(`+)(?!`)([\s\S]*?[^`])\1(?!`)/g;
const INLINE_LINK =
	/(!?)\[((?:[^[\]\\\n]|\\.|\[[^\]\n]*\])*)\]\(\s*(<[^>\n]*>|[^\s()]*(?:\([^\s()]*\)[^\s()]*)*)((?:\s+(?:"[^"\n]*"|'[^'\n]*'))?)\s*\)/g;
const WIKILINK = /(!?)\[\[([^\]\n|]+)(?:\|([^\]\n]*))?\]\]/g;
const DEFINITION = /^( {0,3}\[[^\]\n]+\]:[ \t]*)(<[^>\n]*>|\S+)([^\n]*)$/gm;

const unwrap = (url: string): string =>
	url.startsWith('<') && url.endsWith('>') ? url.slice(1, -1) : url;

const wrapUrl = (url: string): string =>
	/[\s()]/.test(url) ? `<${url}>` : url;

const escapeLabel = (label: string): string =>
	label.replace(/([[\]])/g, '\\$1');

const rewriteText = (text: string, fn: LinkRewriter): string => {
	let result = text.replace(
		INLINE_LINK,
		(whole, bang: string, label: string, url: string, title: string) => {
			const kind: LinkKind = bang ? 'image' : 'link';
			const replacement = fn({
				kind,
				label,
				target: unwrap(url),
				text: whole,
				build: (target, nextLabel) =>
					`${bang}[${nextLabel ?? label}](${wrapUrl(target)}${title})`,
			});
			return replacement ?? whole;
		},
	);
	result = result.replace(
		WIKILINK,
		(whole, bang: string, name: string, alias: string | undefined) => {
			const replacement = fn({
				kind: bang ? 'embed' : 'wikilink',
				label: alias ?? '',
				target: name.trim(),
				text: whole,
				build: (target, nextLabel) =>
					`${bang}[${escapeLabel(nextLabel ?? alias ?? name.trim())}](${wrapUrl(
						target,
					)})`,
			});
			return replacement ?? whole;
		},
	);
	return result.replace(
		DEFINITION,
		(whole, prefix: string, url: string, rest: string) => {
			const replacement = fn({
				kind: 'definition',
				label: '',
				target: unwrap(url),
				text: whole,
				build: (target) => `${prefix}${wrapUrl(target)}${rest}`,
			});
			return replacement ?? whole;
		},
	);
};

const rewriteOutsideInlineCode = (text: string, fn: LinkRewriter): string => {
	let out = '';
	let last = 0;
	for (const match of text.matchAll(INLINE_CODE)) {
		out += rewriteText(text.slice(last, match.index), fn) + match[0];
		last = match.index + match[0].length;
	}
	return out + rewriteText(text.slice(last), fn);
};

export const rewriteMarkdownLinks = (
	markdown: string,
	fn: LinkRewriter,
): string => {
	const lines = markdown.split('\n');
	const chunks: { code: boolean; lines: string[] }[] = [];
	let fence: { char: string; length: number } | null = null;
	let current: { code: boolean; lines: string[] } | null = null;
	const push = (code: boolean, line: string) => {
		if (!current || current.code !== code) {
			current = { code, lines: [] };
			chunks.push(current);
		}
		current.lines.push(line);
	};
	for (const line of lines) {
		if (fence) {
			push(true, line);
			const close = /^ {0,3}(`{3,}|~{3,})[ \t]*$/.exec(line);
			if (
				close &&
				close[1][0] === fence.char &&
				close[1].length >= fence.length
			) {
				fence = null;
			}
			continue;
		}
		const open = FENCE_OPEN.exec(line);
		if (open) {
			fence = { char: open[1][0], length: open[1].length };
			push(true, line);
			continue;
		}
		push(false, line);
	}
	return chunks
		.map((chunk) => {
			const text = chunk.lines.join('\n');
			return chunk.code ? text : rewriteOutsideInlineCode(text, fn);
		})
		.join('\n');
};

export const collectLinks = (markdown: string): LinkToken[] => {
	const tokens: LinkToken[] = [];
	rewriteMarkdownLinks(markdown, (token) => {
		tokens.push(token);
		return null;
	});
	return tokens;
};

const splitFragment = (url: string): { base: string; fragment: string } => {
	const index = url.indexOf('#');
	return index === -1
		? { base: url, fragment: '' }
		: { base: url.slice(0, index), fragment: url.slice(index) };
};

export interface ExportLinkContext {
	fromPath: string;
	workspaceCode: string;
	pagePaths: Map<number, string>;
	pageSlugPaths: Map<string, string>;
	filePaths: Map<number, string>;
}

export const rewriteForExport = (
	body: string,
	ctx: ExportLinkContext,
): string =>
	rewriteMarkdownLinks(body, (token) => {
		if (token.kind !== 'link' && token.kind !== 'image') return null;
		const { base, fragment } = splitFragment(token.target);
		const parsed = parseTmgrUrl(base);
		if (!parsed) return null;
		let path: string | undefined;
		if (parsed.form === 'deep') {
			if (parsed.workspace === ctx.workspaceCode) {
				path = ctx.pageSlugPaths.get(parsed.slug);
			}
		} else if (parsed.kind === 'page') {
			path = ctx.pagePaths.get(Number(parsed.id));
		} else if (parsed.kind === 'file') {
			path = ctx.filePaths.get(Number(parsed.id));
		}
		if (!path) return null;
		const isPage = path.endsWith('.md');
		return token.build(
			encodePath(relativePath(ctx.fromPath, path)) + (isPage ? fragment : ''),
		);
	});

export const isTmgrTarget = (target: string): boolean =>
	/^tmgr:/i.test(target.trim());

export const isExternalTarget = (target: string): boolean =>
	/^[A-Za-z][A-Za-z0-9+.-]*:/.test(target) || target.startsWith('//');

export const splitTargetFragment = splitFragment;
