import { Marked, type Token, type Tokens } from 'marked';

export interface MarkdownOptions {
	/** Key prefixes used by this workspace's categories, e.g. ["TM", "TMBE"]. */
	taskKeyPrefixes?: string[];
}

interface TaskKeyToken extends Tokens.Generic {
	type: 'taskKey';
	raw: string;
	key: string;
}

const baseOptions = {
	gfm: true,
	breaks: true,
	async: false,
} as const;

/** A key used as a link label stays the label: no button inside an anchor. */
const unwrapTaskKeys = (html: string): string =>
	html.replace(/<button[^>]*data-task-key="[^"]*"[^>]*>(.*?)<\/button>/g, '$1');

const linkRenderer = {
	// Comment links point outward; keep the app open and deny the new page a
	// handle on the one that opened it.
	link({ href, title, tokens }: Tokens.Link): string {
		// eslint-disable-next-line @typescript-eslint/no-explicit-any
		const text = unwrapTaskKeys((this as any).parser.parseInline(tokens));
		const titleAttr = title ? ` title="${title}"` : '';
		return `<a href="${href}"${titleAttr} target="_blank" rel="noopener noreferrer">${text}</a>`;
	},
};

const plain = new Marked({ ...baseOptions, renderer: linkRenderer });

/**
 * A task key ("TM-129") written in a comment is how people point at a task, so
 * it is rendered as something clickable. Only prefixes the workspace actually
 * uses are matched — otherwise UTF-8 and GPT-4 would become links too.
 *
 * It is a marked extension rather than a pass over the finished HTML: marked
 * only offers text tokens to an inline extension, so keys inside code spans,
 * fenced blocks, link labels and URLs are left alone for free.
 */
const withTaskKeys = (prefixes: string[]) => {
	const alternatives = prefixes
		.map((p) => p.replace(/[^A-Za-z0-9]/g, ''))
		.filter(Boolean)
		.join('|');
	const pattern = new RegExp(`^(${alternatives})-(\\d{1,7})\\b`, 'i');
	const search = new RegExp(`(?:^|[^A-Za-z0-9-])(${alternatives})-\\d`, 'i');

	return new Marked({
		...baseOptions,
		renderer: linkRenderer,
		extensions: [
			{
				name: 'taskKey',
				level: 'inline' as const,
				start(src: string) {
					const match = search.exec(src);
					if (!match) {
						return undefined;
					}
					// index of the key itself, not of the character in front of it
					return match.index + match[0].length - match[1].length - 2;
				},
				tokenizer(src: string): TaskKeyToken | undefined {
					const match = pattern.exec(src);
					if (!match) {
						return undefined;
					}
					return {
						type: 'taskKey',
						raw: match[0],
						key: `${match[1].toUpperCase()}-${match[2]}`,
					};
				},
				renderer(token: Token) {
					const key = (token as TaskKeyToken).key;
					return `<button type="button" class="task-key" data-task-key="${key}">${key}</button>`;
				},
			},
		],
	});
};

const byPrefixes = new Map<string, Marked>();

const rendererFor = (prefixes?: string[]): Marked => {
	const clean = (prefixes || []).filter(Boolean);
	if (!clean.length) {
		return plain;
	}
	const cacheKey = clean.join('|').toUpperCase();
	let marked = byPrefixes.get(cacheKey);
	if (!marked) {
		marked = withTaskKeys(clean);
		byPrefixes.set(cacheKey, marked);
	}
	return marked;
};

/**
 * Markdown as it is written in comments and chat: GitHub flavour, and a single
 * newline is a line break, because people type comments like messages, not like
 * documents.
 *
 * The result is raw HTML — marked passes embedded HTML through untouched — so
 * every caller must sanitize it before it reaches the DOM (see sanitizeHtml).
 */
export const markdownToHtml = (text: string, options: MarkdownOptions = {}): string =>
	text ? (rendererFor(options.taskKeyPrefixes).parse(text) as string) : '';
