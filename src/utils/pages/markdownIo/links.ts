import { parseTmgrUrl } from '@/utils/pages/tmgrLinks';
import { encodePath, relativePath } from './paths';

export type LinkKind =
	| 'link'
	| 'image'
	| 'wikilink'
	| 'embed'
	| 'definition'
	| 'autolink'
	| 'reference';

export interface LinkToken {
	kind: LinkKind;
	label: string;
	plain: string;
	target: string;
	text: string;
	build: (target: string, label?: string) => string;
}

export type LinkRewriter = (token: LinkToken) => string | null | undefined;

const FENCE_OPEN = /^ {0,3}(`{3,}|~{3,})/;
const FENCE_CLOSE = /^ {0,3}(`{3,}|~{3,})[ \t]*$/;
const LIST_ITEM = /^ {0,3}(?:[-+*]|\d{1,9}[.)])[ \t]+\S/;
const DEFINITION =
	/^( {0,3}\[(?!\^)([^\]\n]{1,999})\]:[ \t]*)(<[^>\n]*>|\S+)([^\n]*)$/;
const AUTOLINK = /<([A-Za-z][A-Za-z0-9+.-]{1,31}:[^\s<>]{0,2000})>/y;
const MAX_DEPTH = 64;
const MAX_LABEL = 1000;
const MAX_URL = 2048;

const unwrap = (url: string): string =>
	url.startsWith('<') && url.endsWith('>') ? url.slice(1, -1) : url;

const wrapUrl = (url: string): string =>
	/[\s()]/.test(url) ? `<${url}>` : url;

const escapeLabel = (label: string): string =>
	label.replace(/([[\]])/g, '\\$1');

const indentWidth = (line: string): number => {
	let width = 0;
	for (const char of line) {
		if (char === ' ') width += 1;
		else if (char === '\t') width += 4 - (width % 4);
		else break;
	}
	return width;
};

export const escapeBrackets = (text: string): string => {
	let out = '';
	for (let i = 0; i < text.length; i += 1) {
		const char = text[i];
		if (char === '\\' && i + 1 < text.length) {
			out += char + text[i + 1];
			i += 1;
		} else if (char === '[' || char === ']') {
			out += `\\${char}`;
		} else {
			out += char;
		}
	}
	return out;
};

export const normalizeReference = (label: string): string =>
	label.trim().replace(/\s+/g, ' ').toLowerCase();

interface Match {
	kind: LinkKind;
	start: number;
	end: number;
	bang: boolean;
	labelStart: number;
	labelEnd: number;
	target: string;
	title: string;
	prefix: string;
	rest: string;
	children: Match[];
}

interface Frame {
	pos: number;
	bracket: number;
	bang: boolean;
	active: boolean;
	children: Match[];
}

export interface RewriteOptions {
	references?: Set<string>;
}

const leaf = (
	kind: LinkKind,
	start: number,
	end: number,
	target: string,
	extra: Partial<Match> = {},
): Match => ({
	kind,
	start,
	end,
	bang: false,
	labelStart: 0,
	labelEnd: 0,
	target,
	title: '',
	prefix: '',
	rest: '',
	children: [],
	...extra,
});

const parseInline = (
	line: string,
	open: number,
): { target: string; title: string; end: number } | null => {
	const length = line.length;
	let j = open + 1;
	while (j < length && (line[j] === ' ' || line[j] === '\t')) j += 1;
	let raw: string;
	if (line[j] === '<') {
		const close = line.indexOf('>', j + 1);
		if (close === -1 || close - j > MAX_URL) return null;
		raw = line.slice(j, close + 1);
		j = close + 1;
	} else {
		const first = j;
		let depth = 0;
		while (j < length && j - first <= MAX_URL) {
			const char = line[j];
			if (char === '\\' && j + 1 < length) {
				j += 2;
				continue;
			}
			if (char === ' ' || char === '\t') break;
			if (char === '(') {
				depth += 1;
				if (depth > 1) return null;
			} else if (char === ')') {
				if (depth === 0) break;
				depth -= 1;
			}
			j += 1;
		}
		if (j - first > MAX_URL) return null;
		raw = line.slice(first, j);
	}
	const afterUrl = j;
	while (j < length && (line[j] === ' ' || line[j] === '\t')) j += 1;
	let title = '';
	if (j > afterUrl && (line[j] === '"' || line[j] === "'")) {
		const close = line.indexOf(line[j], j + 1);
		if (close === -1 || close - j > MAX_LABEL) return null;
		title = line.slice(afterUrl, close + 1);
		j = close + 1;
		while (j < length && (line[j] === ' ' || line[j] === '\t')) j += 1;
	}
	if (line[j] !== ')') return null;
	return { target: unwrap(raw), title, end: j + 1 };
};

const scanLine = (
	line: string,
	lineStart: boolean,
	options: RewriteOptions,
): Match[] => {
	if (lineStart) {
		const def = DEFINITION.exec(line);
		if (def) {
			return [
				leaf('definition', 0, line.length, unwrap(def[3]), {
					prefix: def[1],
					rest: def[4],
					labelStart: def[1].indexOf('[') + 1,
					labelEnd: def[1].indexOf('[') + 1 + def[2].length,
				}),
			];
		}
	}
	const top: Match[] = [];
	const stack: Frame[] = [];
	let overflow = 0;
	let closeAt = -2;
	const emit = (match: Match) => {
		(stack.length ? stack[stack.length - 1].children : top).push(match);
	};
	const nextClose = (from: number): number => {
		if (closeAt === -1) return -1;
		if (closeAt < from) closeAt = line.indexOf(']]', from);
		return closeAt;
	};
	const length = line.length;
	let i = 0;
	while (i < length) {
		const char = line[i];
		if (char === '\\') {
			i += 2;
		} else if (char === '<') {
			AUTOLINK.lastIndex = i;
			const auto = AUTOLINK.exec(line);
			if (auto) {
				emit(leaf('autolink', i, i + auto[0].length, auto[1]));
				i += auto[0].length;
			} else {
				i += 1;
			}
		} else if (char === '[' || (char === '!' && line[i + 1] === '[')) {
			const bang = char === '!';
			const bracket = bang ? i + 1 : i;
			if (line[bracket + 1] === '[') {
				const close = nextClose(bracket + 2);
				if (close !== -1 && close - bracket <= MAX_LABEL * 2 + 2) {
					const inner = line.slice(bracket + 2, close);
					const bar = inner.indexOf('|');
					const name = bar === -1 ? inner : inner.slice(0, bar);
					if (name && !inner.includes(']')) {
						emit(
							leaf(bang ? 'embed' : 'wikilink', i, close + 2, name.trim(), {
								bang,
								rest: bar === -1 ? '' : inner.slice(bar + 1),
								prefix: name,
								title: bar === -1 ? '' : '|',
							}),
						);
						i = close + 2;
						continue;
					}
				}
			}
			if (stack.length >= MAX_DEPTH) overflow += 1;
			else {
				stack.push({
					pos: i,
					bracket,
					bang,
					active: true,
					children: [],
				});
			}
			i = bracket + 1;
		} else if (char === ']') {
			if (overflow > 0) {
				overflow -= 1;
				i += 1;
				continue;
			}
			const frame = stack.pop();
			if (!frame) {
				i += 1;
				continue;
			}
			let match: Match | null = null;
			if ((frame.active || frame.bang) && i - frame.bracket <= MAX_LABEL + 1) {
				const labelBase = {
					bang: frame.bang,
					labelStart: frame.bracket + 1,
					labelEnd: i,
					children: frame.children,
				};
				if (line[i + 1] === '(') {
					const inline = parseInline(line, i + 1);
					if (inline) {
						match = leaf(
							frame.bang ? 'image' : 'link',
							frame.pos,
							inline.end,
							inline.target,
							{ ...labelBase, title: inline.title },
						);
					}
				}
				const refs = options.references;
				if (!match && refs && refs.size) {
					const rawLabel = line.slice(frame.bracket + 1, i);
					let end = i + 1;
					let ref = rawLabel;
					if (line[i + 1] === '[') {
						const close = line.indexOf(']', i + 2);
						if (close !== -1 && close - i <= MAX_LABEL + 1) {
							const inner = line.slice(i + 2, close);
							if (!inner.includes('[')) {
								ref = inner || rawLabel;
								end = close + 1;
							}
						}
					}
					if (refs.has(normalizeReference(ref))) {
						match = leaf(
							'reference',
							frame.pos,
							end,
							normalizeReference(ref),
							labelBase,
						);
					}
				}
			}
			if (match) {
				if (!frame.bang) for (const below of stack) below.active = false;
				emit(match);
				i = match.end;
			} else {
				for (const child of frame.children) emit(child);
				i += 1;
			}
		} else {
			i += 1;
		}
	}
	while (stack.length) {
		const frame = stack.pop() as Frame;
		for (const child of frame.children) emit(child);
	}
	return top;
};

type Rendered = { text: string; plain: string };

const renderRange = (
	line: string,
	from: number,
	to: number,
	matches: Match[],
	fn: LinkRewriter,
): Rendered => {
	let text = '';
	let plain = '';
	let pos = from;
	for (const match of matches) {
		const literal = line.slice(pos, match.start);
		text += literal;
		plain += escapeBrackets(literal);
		const out = renderMatch(line, match, fn);
		text += out;
		plain += out;
		pos = match.end;
	}
	const tail = line.slice(pos, to);
	return { text: text + tail, plain: plain + escapeBrackets(tail) };
};

const renderMatch = (line: string, match: Match, fn: LinkRewriter): string => {
	const whole = line.slice(match.start, match.end);
	const bang = match.bang ? '!' : '';
	let token: LinkToken;
	let rendered: Rendered | null = null;
	if (
		match.kind === 'link' ||
		match.kind === 'image' ||
		match.kind === 'reference'
	) {
		rendered = renderRange(
			line,
			match.labelStart,
			match.labelEnd,
			match.children,
			fn,
		);
		const label = rendered.text;
		token = {
			kind: match.kind,
			label,
			plain: rendered.plain,
			target: match.target,
			text: whole,
			build: (target, nextLabel) =>
				match.kind === 'reference'
					? whole
					: `${bang}[${nextLabel ?? label}](${wrapUrl(target)}${match.title})`,
		};
	} else if (match.kind === 'definition') {
		const label = line.slice(match.labelStart, match.labelEnd);
		token = {
			kind: 'definition',
			label,
			plain: '',
			target: match.target,
			text: whole,
			build: (target) => `${match.prefix}${wrapUrl(target)}${match.rest}`,
		};
	} else if (match.kind === 'autolink') {
		token = {
			kind: 'autolink',
			label: '',
			plain: '',
			target: match.target,
			text: whole,
			build: (target) => `<${target}>`,
		};
	} else {
		const alias = match.title ? match.rest : '';
		token = {
			kind: match.kind,
			label: alias,
			plain: escapeBrackets(alias),
			target: match.target,
			text: whole,
			build: (target, nextLabel) =>
				`${bang}[${escapeLabel(
					nextLabel ?? (match.title ? match.rest : match.prefix.trim()),
				)}](${wrapUrl(target)})`,
		};
	}
	const replacement = fn(token);
	if (replacement !== null && replacement !== undefined) return replacement;
	if (
		rendered &&
		rendered.text !== line.slice(match.labelStart, match.labelEnd)
	) {
		return (
			line.slice(match.start, match.labelStart) +
			rendered.text +
			line.slice(match.labelEnd, match.end)
		);
	}
	return whole;
};

const rewriteProse = (
	text: string,
	atLineStart: boolean,
	fn: LinkRewriter,
	options: RewriteOptions,
): string => {
	if (!/[[<]/.test(text)) return text;
	const lines = text.split('\n');
	return lines
		.map((line, index) => {
			const matches = scanLine(line, index > 0 || atLineStart, options);
			return matches.length
				? renderRange(line, 0, line.length, matches, fn).text
				: line;
		})
		.join('\n');
};

const inlineCodeRanges = (paragraph: string): [number, number][] => {
	if (!paragraph.includes('`')) return [];
	const runs: { start: number; length: number; escaped: boolean }[] = [];
	for (const match of paragraph.matchAll(/`+/g)) {
		let slashes = 0;
		for (let k = match.index - 1; k >= 0 && paragraph[k] === '\\'; k -= 1) {
			slashes += 1;
		}
		runs.push({
			start: match.index,
			length: match[0].length,
			escaped: slashes % 2 === 1,
		});
	}
	const byLength = new Map<number, number[]>();
	runs.forEach((run, index) => {
		const list = byLength.get(run.length) ?? [];
		list.push(index);
		byLength.set(run.length, list);
	});
	const pointers = new Map<number, number>();
	const ranges: [number, number][] = [];
	let i = 0;
	while (i < runs.length) {
		const run = runs[i];
		const length = run.escaped ? run.length - 1 : run.length;
		const start = run.escaped ? run.start + 1 : run.start;
		const list = length > 0 ? byLength.get(length) : undefined;
		if (!list) {
			i += 1;
			continue;
		}
		let pointer = pointers.get(length) ?? 0;
		while (pointer < list.length && list[pointer] <= i) pointer += 1;
		pointers.set(length, pointer);
		if (pointer >= list.length) {
			i += 1;
			continue;
		}
		const close = runs[list[pointer]];
		ranges.push([start, close.start + close.length]);
		i = list[pointer] + 1;
	}
	return ranges;
};

const paragraphRanges = (text: string): [number, number][] => {
	const ranges: [number, number][] = [];
	let start = 0;
	let position = 0;
	for (const line of text.split('\n')) {
		if (line.trim() === '') {
			if (position > start) ranges.push([start, position]);
			start = position + line.length + 1;
		}
		position += line.length + 1;
	}
	if (start < text.length) ranges.push([start, text.length]);
	return ranges;
};

const rewriteParagraphs = (
	text: string,
	fn: LinkRewriter,
	options: RewriteOptions,
): string => {
	let out = '';
	let last = 0;
	const flush = (until: number) => {
		if (until <= last) return;
		out += rewriteProse(
			text.slice(last, until),
			last === 0 || text[last - 1] === '\n',
			fn,
			options,
		);
	};
	for (const [from, to] of paragraphRanges(text)) {
		for (const [start, end] of inlineCodeRanges(text.slice(from, to))) {
			flush(from + start);
			out += text.slice(from + start, from + end);
			last = from + end;
		}
	}
	flush(text.length);
	return out;
};

export const rewriteMarkdownLinks = (
	markdown: string,
	fn: LinkRewriter,
	options: RewriteOptions = {},
): string => {
	const chunks: { code: boolean; lines: string[] }[] = [];
	let fence: { char: string; length: number } | null = null;
	let current: { code: boolean; lines: string[] } | null = null;
	let previousBlank = true;
	let indentedCode = false;
	let inList = false;
	const push = (code: boolean, line: string) => {
		if (!current || current.code !== code) {
			current = { code, lines: [] };
			chunks.push(current);
		}
		current.lines.push(line);
	};
	for (const line of markdown.split('\n')) {
		if (fence) {
			push(true, line);
			const close = FENCE_CLOSE.exec(line);
			if (
				close &&
				close[1][0] === fence.char &&
				close[1].length >= fence.length
			) {
				fence = null;
			}
			continue;
		}
		const blank = line.trim() === '';
		if (
			!blank &&
			indentWidth(line) >= 4 &&
			(previousBlank || indentedCode) &&
			!inList
		) {
			indentedCode = true;
			previousBlank = false;
			push(true, line);
			continue;
		}
		if (blank) {
			if (indentedCode) {
				push(true, line);
				continue;
			}
		} else {
			indentedCode = false;
		}
		const open = FENCE_OPEN.exec(line);
		if (open) {
			fence = { char: open[1][0], length: open[1].length };
			push(true, line);
			previousBlank = false;
			continue;
		}
		if (!blank) {
			if (LIST_ITEM.test(line)) inList = true;
			else if (previousBlank && indentWidth(line) < 4) inList = false;
		}
		previousBlank = blank;
		push(false, line);
	}
	return chunks
		.map((chunk) => {
			const text = chunk.lines.join('\n');
			return chunk.code ? text : rewriteParagraphs(text, fn, options);
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
		if (
			token.kind !== 'link' &&
			token.kind !== 'image' &&
			token.kind !== 'definition'
		) {
			return null;
		}
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
