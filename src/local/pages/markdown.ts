export const MAX_BODY_BYTES = 1_048_576;
export const MAX_TASK_KEYS = 50;
export const AGENTS_OWNER = 'agents';
export const SECTION_CLOSE = '<!-- /tmgr:section -->';

export interface LinkRef {
	kind: string;
	id: number;
}

export interface Heading {
	level: number;
	text: string;
	start: number;
	end: number;
}

/** `start..end` spans both markers; `innerStart..innerEnd` is what a section write replaces. */
export interface Section {
	id: string;
	owner: string;
	heading: string | null;
	start: number;
	innerStart: number;
	innerEnd: number;
	end: number;
}

const TMGR_LINK = /tmgr:\/\/(page|task|category|user|persona)\/(\d+)/g;
const TASK_KEY = /(?<![\p{L}\p{N}_/-])([A-Z][A-Z0-9]+-\d+)(?![\p{L}\p{N}_-])/gu;
const SECTION_OPEN = /<!--\s*tmgr:section\s+id="([^"]+)"\s+owner="([^"]+)"\s*-->/g;
const SECTION_END = /<!--\s*\/tmgr:section\s*-->/g;
const FENCE = /^ {0,3}(`{3,}|~{3,})/;
const BARE_URL = /(?:https?|tmgr):\/\/[^\s)>\]]+/g;
const LIST_LINE = /^\s*(?:[-*+]|\d+[.)])\s+\S.*$/;
const ENCODER = new TextEncoder();

export const byteLength = (body: string): number => ENCODER.encode(body).length;

export const exceedsLimit = (body: string): boolean => byteLength(body) > MAX_BODY_BYTES;

const fill = (skip: Uint8Array, from: number, to: number) => {
	for (let i = from; i < to && i < skip.length; i++) skip[i] = 1;
};

const isFenceClose = (line: string, fence: RegExpExecArray, char: string, len: number): boolean =>
	fence[1][0] === char && fence[1].length >= len && line.trim().length === fence[1].length;

const maskFences = (body: string, skip: Uint8Array) => {
	let pos = 0;
	let fenceChar = '';
	let fenceLen = 0;
	let fenceStart = 0;
	while (pos < body.length) {
		const eol = body.indexOf('\n', pos);
		const end = eol < 0 ? body.length : eol;
		const line = body.substring(pos, end);
		const m = FENCE.exec(line);
		if (!fenceChar) {
			if (m) {
				fenceChar = m[1][0];
				fenceLen = m[1].length;
				fenceStart = pos;
			}
		} else if (m && isFenceClose(line, m, fenceChar, fenceLen)) {
			fill(skip, fenceStart, end);
			fenceChar = '';
		}
		pos = end + 1;
	}
	if (fenceChar) fill(skip, fenceStart, body.length);
};

const maskMatches = (pattern: RegExp, body: string, skip: Uint8Array) => {
	const re = new RegExp(pattern.source, pattern.flags);
	let m: RegExpExecArray | null;
	while ((m = re.exec(body))) {
		if (!skip[m.index]) fill(skip, m.index, m.index + m[0].length);
		if (m[0].length === 0) re.lastIndex++;
	}
};

const maskComments = (body: string, skip: Uint8Array) => {
	let i = 0;
	while ((i = body.indexOf('<!--', i)) >= 0) {
		const end = body.indexOf('-->', i + 4);
		if (end < 0) return;
		if (!skip[i]) fill(skip, i, end + 3);
		i = end + 3;
	}
};

const positionsOf = (body: string, needle: string): number[] => {
	const out: number[] = [];
	for (let i = body.indexOf(needle); i >= 0; i = body.indexOf(needle, i + 1)) out.push(i);
	return out;
};

const maskInlineCode = (body: string, skip: Uint8Array) => {
	const n = body.length;
	const starts: number[] = [];
	const lens: number[] = [];
	for (let i = 0; i < n; ) {
		if (body[i] !== '`') {
			i++;
			continue;
		}
		let j = i;
		while (j < n && body[j] === '`') j++;
		starts.push(i);
		lens.push(j - i);
		i = j;
	}
	const runs = starts.length;
	if (runs < 2) return;
	const nextSame = new Array<number>(runs);
	const last = new Map<number, number>();
	for (let k = runs - 1; k >= 0; k--) {
		nextSame[k] = last.get(lens[k]) ?? -1;
		last.set(lens[k], k);
	}
	const blanks = positionsOf(body, '\n\n');
	let b = 0;
	for (let k = 0; k < runs; k++) {
		const j = nextSame[k];
		if (j < 0 || skip[starts[k]]) continue;
		const closeEnd = starts[j] + lens[j];
		while (b < blanks.length && blanks[b] < starts[k]) b++;
		if (b < blanks.length && blanks[b] + 2 <= closeEnd) continue;
		fill(skip, starts[k], closeEnd);
		k = j;
	}
};

const nextOf = (body: string, ch: string): Int32Array => {
	const n = body.length;
	const next = new Int32Array(n + 2);
	next[n] = -1;
	next[n + 1] = -1;
	for (let i = n - 1; i >= 0; i--) next[i] = body[i] === ch ? i : next[i + 1];
	return next;
};

const maskMarkdownLinks = (body: string, skip: Uint8Array) => {
	if (body.indexOf('[') < 0) return;
	const n = body.length;
	const newline = nextOf(body, '\n');
	const bracket = nextOf(body, ']');
	const paren = nextOf(body, ')');
	for (let i = 0; i < n; i++) {
		if (body[i] !== '[') continue;
		const close = bracket[i + 1];
		if (close < 0 || (newline[i + 1] >= 0 && newline[i + 1] < close) || close + 1 >= n || body[close + 1] !== '(') {
			continue;
		}
		const end = paren[close + 2];
		if (end < 0 || (newline[close + 2] >= 0 && newline[close + 2] < end)) continue;
		const start = i > 0 && body[i - 1] === '!' ? i - 1 : i;
		if (!skip[start]) fill(skip, start, end + 1);
		i = end;
	}
};

const isAsciiLetter = (c: string) => (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z');
const isDigit = (c: string) => c >= '0' && c <= '9';
const isSpace = (c: string) => /\s/.test(c);

const maskAngleAutolinks = (body: string, skip: Uint8Array) => {
	const n = body.length;
	for (let i = body.indexOf('<'); i >= 0 && i < n; i = body.indexOf('<', i + 1)) {
		let j = i + 1;
		if (j >= n || !isAsciiLetter(body[j])) continue;
		while (j < n && (isAsciiLetter(body[j]) || isDigit(body[j]) || '+.-'.includes(body[j]))) j++;
		if (j >= n || body[j] !== ':') continue;
		let k = j + 1;
		while (k < n && body[k] !== '>' && body[k] !== '<' && !isSpace(body[k])) k++;
		if (k < n && body[k] === '>' && !skip[i]) fill(skip, i, k + 1);
	}
};

const mask = (body: string, includeLinks: boolean): Uint8Array => {
	const skip = new Uint8Array(body.length + 1);
	maskFences(body, skip);
	maskComments(body, skip);
	maskInlineCode(body, skip);
	if (includeLinks) {
		maskMarkdownLinks(body, skip);
		maskAngleAutolinks(body, skip);
		maskMatches(BARE_URL, body, skip);
	}
	return skip;
};

export const extractLinks = (body: string): LinkRef[] => {
	const skip = mask(body, false);
	const seen = new Set<string>();
	const out: LinkRef[] = [];
	const re = new RegExp(TMGR_LINK.source, 'g');
	let m: RegExpExecArray | null;
	while ((m = re.exec(body))) {
		if (skip[m.index]) continue;
		const id = Number(m[2]);
		if (!Number.isSafeInteger(id)) continue;
		const key = `${m[1]}:${id}`;
		if (seen.has(key)) continue;
		seen.add(key);
		out.push({ kind: m[1], id });
	}
	return out;
};

/** Bare task keys (TM-212) outside code, comments and existing links, in order of appearance. */
export const taskKeyCandidates = (body: string): string[] => {
	const skip = mask(body, true);
	const out = new Set<string>();
	const re = new RegExp(TASK_KEY.source, TASK_KEY.flags);
	let m: RegExpExecArray | null;
	while ((m = re.exec(body)) && out.size < MAX_TASK_KEYS) {
		if (!skip[m.index]) out.add(m[1]);
	}
	return [...out];
};

export const autolinkTaskKeys = (body: string, taskIdByKey: Map<string, number>): string => {
	if (!taskIdByKey.size) return body;
	const skip = mask(body, true);
	const re = new RegExp(TASK_KEY.source, TASK_KEY.flags);
	let out = '';
	let last = 0;
	let m: RegExpExecArray | null;
	while ((m = re.exec(body))) {
		const id = skip[m.index] ? undefined : taskIdByKey.get(m[1]);
		if (id === undefined) continue;
		out += `${body.slice(last, m.index)}[${m[1]}](tmgr://task/${id})`;
		last = m.index + m[0].length;
	}
	return out + body.slice(last);
};

const parseHeading = (line: string, start: number, end: number): Heading | null => {
	const len = line.length;
	let i = 0;
	while (i < 3 && i < len && line[i] === ' ') i++;
	let hashes = 0;
	while (i + hashes < len && line[i + hashes] === '#') hashes++;
	const p = i + hashes;
	if (hashes < 1 || hashes > 6 || p >= len || (line[p] !== ' ' && line[p] !== '\t')) return null;
	let text = line.substring(p).trim();
	if (text.endsWith('#')) {
		let r = text.length;
		while (r > 0 && text[r - 1] === '#') r--;
		if (r === 0 || text[r - 1] === ' ' || text[r - 1] === '\t') text = text.substring(0, r).trim();
	}
	return { level: hashes, text, start, end };
};

export const headings = (body: string): Heading[] => {
	const out: Heading[] = [];
	let pos = 0;
	let fenceChar = '';
	let fenceLen = 0;
	while (pos <= body.length) {
		const eol = body.indexOf('\n', pos);
		const end = eol < 0 ? body.length : eol;
		const line = body.substring(pos, end);
		const f = FENCE.exec(line);
		if (!fenceChar) {
			if (f) {
				fenceChar = f[1][0];
				fenceLen = f[1].length;
			} else {
				const h = parseHeading(line, pos, end);
				if (h) out.push(h);
			}
		} else if (f && isFenceClose(line, f, fenceChar, fenceLen)) {
			fenceChar = '';
		}
		if (eol < 0) break;
		pos = end + 1;
	}
	return out;
};

interface Marker {
	open: boolean;
	id: string | null;
	owner: string | null;
	start: number;
	end: number;
}

interface Parsed {
	sections: Section[];
	opens: Marker[];
	error: string | null;
}

const firstHeadingText = (inner: string): string | null => headings(inner)[0]?.text ?? null;

const parse = (body: string): Parsed => {
	const skip = new Uint8Array(body.length + 1);
	maskFences(body, skip);
	maskInlineCode(body, skip);
	const markers: Marker[] = [];
	const open = new RegExp(SECTION_OPEN.source, 'g');
	let m: RegExpExecArray | null;
	while ((m = open.exec(body))) {
		if (!skip[m.index]) {
			markers.push({ open: true, id: m[1], owner: m[2], start: m.index, end: m.index + m[0].length });
		}
	}
	const close = new RegExp(SECTION_END.source, 'g');
	while ((m = close.exec(body))) {
		if (!skip[m.index]) {
			markers.push({ open: false, id: null, owner: null, start: m.index, end: m.index + m[0].length });
		}
	}
	markers.sort((a, b) => a.start - b.start);
	const sections: Section[] = [];
	const opens: Marker[] = [];
	const ids = new Set<string>();
	let error: string | null = null;
	let pending: Marker | null = null;
	for (const marker of markers) {
		if (marker.open) {
			opens.push(marker);
			if (pending && error === null) {
				error = `Section '${pending.id}' is not closed before the next section starts`;
			}
			pending = marker;
		} else if (!pending) {
			error = error ?? 'A section closing marker has no opening marker';
		} else {
			const inner = body.substring(pending.end, marker.start);
			if (ids.has(pending.id!) && error === null) error = `Section id '${pending.id}' is used more than once`;
			ids.add(pending.id!);
			sections.push({
				id: pending.id!,
				owner: pending.owner!,
				heading: firstHeadingText(inner),
				start: pending.start,
				innerStart: pending.end,
				innerEnd: marker.start,
				end: marker.end,
			});
			pending = null;
		}
	}
	if (pending && error === null) error = `Section '${pending.id}' is never closed`;
	return { sections, opens, error };
};

export const sections = (body: string): Section[] => parse(body).sections;

/** Why the section markers of this body are not well formed: duplicate ids, nesting, strays. */
export const structureError = (body: string): string | null => parse(body).error;

/** Whether text supplied by an agent carries a section marker of any kind. */
export const containsSectionMarker = (text: string): boolean => {
	for (let i = text.indexOf('<!--'); i >= 0; i = text.indexOf('<!--', i + 1)) {
		let j = i + 4;
		while (j < text.length && isSpace(text[j])) j++;
		if (text.startsWith('tmgr:section', j) || text.startsWith('/tmgr:section', j)) return true;
	}
	return false;
};

const markerKeys = (body: string): string[] => parse(body).opens.map((m) => `${m.id}\u0000${m.owner}`);

export const findSection = (body: string, sectionId: string): Section | null =>
	sections(body).find((s) => s.id === sectionId) ?? null;

/** A persona or plugin may write the shared `agents` owner or its own section; on a context page only `agents`. */
export const writableBy = (owner: string, actorKind: string, actorRef: string, contextPage: boolean): boolean => {
	if (owner === AGENTS_OWNER) return true;
	return !contextPage && owner === `${actorKind}:${actorRef}`;
};

/** Replaces what is between the markers, keeping the section heading when the new text brings none. */
export const replaceSection = (body: string, section: Section, markdown: string): string => {
	const inner = body.substring(section.innerStart, section.innerEnd);
	const found = headings(inner);
	let keptHeading: string | null = null;
	if (found.length && inner.substring(0, found[0].start).trim() === '') {
		keptHeading = inner.substring(found[0].start, found[0].end);
	}
	const text = markdown.trim();
	const firstLine = text.split(/\r\n|\r|\n/)[0] ?? '';
	const bringsHeading = text !== '' && headings(firstLine).length > 0;
	let next = '\n';
	if (keptHeading !== null && !bringsHeading) {
		next += `${keptHeading}\n`;
		if (text) next += '\n';
	}
	if (text) next += `${text}\n`;
	return body.substring(0, section.innerStart) + next + body.substring(section.innerEnd);
};

const rstrip = (s: string): string => {
	let end = s.length;
	while (end > 0 && /\s/.test(s[end - 1])) end--;
	return s.substring(0, end);
};

const insertAt = (body: string, at: number, markdown: string): string => {
	const before = rstrip(body.substring(0, at));
	const after = body.substring(at);
	const text = markdown.trim();
	if (!before) return `${text}\n${after ? `\n${after}` : ''}`;
	const lastLine = before.substring(before.lastIndexOf('\n') + 1);
	const firstLine = text.split(/\r\n|\r|\n/)[0] ?? '';
	const sep = LIST_LINE.test(lastLine) && LIST_LINE.test(firstLine) ? '\n' : '\n\n';
	if (!after) return `${before}${sep}${text}\n`;
	const gap = new RegExp(`^(?:${SECTION_END.source})`).test(after) ? '\n' : '\n\n';
	return `${before}${sep}${text}${gap}${after}`;
};

export const appendToEnd = (body: string, markdown: string): string => insertAt(body, body.length, markdown);

const normalizeHeading = (heading: string | null | undefined): string => {
	let h = (heading ?? '').trim();
	while (h.startsWith('#')) h = h.substring(1);
	return h.trim();
};

/** Appends inside the first `##` section called `heading`; null when there is none. */
export const appendUnderHeading = (body: string, heading: string, markdown: string): string | null => {
	const wanted = normalizeHeading(heading).toLowerCase();
	const all = headings(body);
	const targetIdx = all.findIndex((h) => h.level === 2 && h.text.toLowerCase() === wanted);
	if (targetIdx < 0) return null;
	const target = all[targetIdx];
	let boundary = body.length;
	for (let i = targetIdx + 1; i < all.length; i++) {
		if (all[i].level <= 2) {
			boundary = all[i].start;
			break;
		}
	}
	for (const s of sections(body)) {
		for (const marker of [s.start, s.innerEnd]) {
			if (marker > target.end && marker < boundary) boundary = marker;
		}
	}
	return insertAt(body, boundary, markdown);
};

export const appendNewHeading = (body: string, heading: string, markdown: string): string => {
	const title = normalizeHeading(heading);
	const prefix = body.trim() === '' ? '' : `${rstrip(body)}\n\n`;
	return `${prefix}## ${title}\n\n${markdown.trim()}\n`;
};

/** Appends at the end of a section's content, before its closing marker. */
export const appendInSection = (body: string, section: Section, markdown: string): string =>
	insertAt(body, section.innerEnd, markdown);

const maskWritable = (
	body: string,
	list: Section[],
	actorKind: string,
	actorRef: string,
	contextPage: boolean,
): string => {
	let out = '';
	let last = 0;
	for (const s of list) {
		if (s.start < last || !writableBy(s.owner, actorKind, actorRef, contextPage)) continue;
		out += `${body.slice(last, s.innerStart)}\u0000`;
		last = s.innerEnd;
	}
	return out + body.slice(last);
};

/**
 * Why a persona or plugin may not turn `oldBody` into `newBody`, or null when the change stays inside
 * what it may write: its own (or `agents`) sections, and on a context page nothing else.
 */
export const nonHumanViolation = (
	oldBody: string,
	newBody: string,
	contextPage: boolean,
	actorKind: string,
	actorRef: string,
): string | null => {
	const oldKeys = markerKeys(oldBody);
	const newKeys = markerKeys(newBody);
	if (oldKeys.length !== newKeys.length || oldKeys.some((key, i) => key !== newKeys[i])) {
		return 'Section markers cannot be added, removed or changed';
	}
	const before = sections(oldBody);
	const after = sections(newBody);
	const afterById = new Map<string, Section>();
	for (const s of after) if (!afterById.has(s.id)) afterById.set(s.id, s);
	const beforeIds = new Set<string>();
	for (const s of before) {
		beforeIds.add(s.id);
		const writable = writableBy(s.owner, actorKind, actorRef, contextPage);
		const now = afterById.get(s.id);
		if (!now) return `Section '${s.id}' cannot be removed`;
		if (now.owner !== s.owner) return `Section '${s.id}' owner cannot be changed`;
		if (!writable && oldBody.substring(s.start, s.end) !== newBody.substring(now.start, now.end)) {
			return `Section '${s.id}' is managed by ${s.owner}`;
		}
	}
	for (const s of after) {
		if (!beforeIds.has(s.id) && !writableBy(s.owner, actorKind, actorRef, contextPage)) {
			return `Section '${s.id}' would be owned by ${s.owner}`;
		}
	}
	if (
		contextPage &&
		maskWritable(oldBody, before, actorKind, actorRef, contextPage) !==
			maskWritable(newBody, after, actorKind, actorRef, contextPage)
	) {
		return 'A context page may only be changed inside its agent sections';
	}
	return null;
};

export const PLAIN = 'plain';
export const CONTEXT = 'context';
export const PERSON = 'person';
export const MEETING = 'meeting';
export const PROMISES_SECTION = 'promises';
export const CHRONICLE_HEADING = 'Хронология';
export const CONTEXT_TITLE = 'Контекст воркспейса';

const CONTEXT_BODY = `## Как мы работаем

## Архитектура

<!-- tmgr:section id="agent-notes" owner="agents" -->
## Заметки агентов
<!-- /tmgr:section -->
`;

const MEETING_BODY = `## Повестка

## Итоги

## Решения

## Действия
`;

const PERSON_HEAD = `## Кратко

<!-- tmgr:section id="promises" owner="system" -->
## Обещания
<!-- /tmgr:section -->

## Хронология

## Что я знаю

`;

export const isKnownType = (type: string): boolean =>
	type === PLAIN || type === CONTEXT || type === PERSON || type === MEETING;

export const templateBody = (type: string): string => {
	if (type === CONTEXT) return CONTEXT_BODY;
	if (type === MEETING) return MEETING_BODY;
	if (type === PERSON) return `${PERSON_HEAD}## Инсайты\n`;
	return '';
};
