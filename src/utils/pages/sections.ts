export interface FreeSegment {
	kind: 'free';
	text: string;
}

export interface SectionSegment {
	kind: 'section';
	id: string;
	owner: string;
	heading: string | null;
	open: string;
	inner: string;
	close: string;
}

export type Segment = FreeSegment | SectionSegment;

const OPEN =
	/^<!--\s*tmgr:section\s+id="([^"]+)"\s+owner="([^"]+)"\s*-->[ \t]*\r?\n?$/;
const CLOSE = /^<!--\s*\/tmgr:section\s*-->[ \t]*\r?\n?$/;
const FENCE = /^ {0,3}(`{3,}|~{3,})/;
const HEADING = /^#{1,6}\s+(.+?)\s*#*\s*$/;

const toLines = (body: string): string[] => body.split(/(?<=\n)/);

const nextFence = (fence: string | null, line: string): string | null => {
	const match = FENCE.exec(line);
	if (!match) return fence;
	const mark = match[1][0];
	if (fence === null) return mark;
	return fence === mark ? null : fence;
};

const headingOf = (inner: string): string | null => {
	for (const line of toLines(inner)) {
		const match = HEADING.exec(line.replace(/\r?\n$/, ''));
		if (match) return match[1];
	}
	return null;
};

export const splitBody = (body: string): Segment[] => {
	const lines = toLines(body);
	const segments: Segment[] = [];
	let free = '';
	let fence: string | null = null;
	let i = 0;

	const flush = () => {
		if (free) segments.push({ kind: 'free', text: free });
		free = '';
	};

	while (i < lines.length) {
		const line = lines[i];
		const open = fence === null ? OPEN.exec(line) : null;
		if (open) {
			let innerFence: string | null = null;
			let end = -1;
			for (let j = i + 1; j < lines.length; j++) {
				if (innerFence === null && CLOSE.test(lines[j])) {
					end = j;
					break;
				}
				innerFence = nextFence(innerFence, lines[j]);
			}
			if (end !== -1) {
				flush();
				const inner = lines.slice(i + 1, end).join('');
				segments.push({
					kind: 'section',
					id: open[1],
					owner: open[2],
					heading: headingOf(inner),
					open: line,
					inner,
					close: lines[end],
				});
				i = end + 1;
				continue;
			}
		}
		fence = nextFence(fence, line);
		free += line;
		i++;
	}
	flush();
	return segments;
};

export const joinSegments = (segments: Segment[]): string =>
	segments
		.map((s) => (s.kind === 'free' ? s.text : s.open + s.inner + s.close))
		.join('');

export const padSegments = (segments: Segment[]): Segment[] => {
	const padded: Segment[] = [];
	for (const segment of segments) {
		const last = padded[padded.length - 1];
		if (segment.kind === 'section' && (!last || last.kind === 'section')) {
			padded.push({ kind: 'free', text: '' });
		}
		padded.push(segment);
	}
	const tail = padded[padded.length - 1];
	if (!tail || tail.kind === 'section') padded.push({ kind: 'free', text: '' });
	return padded;
};

export const applyFreeEdit = (
	segments: Segment[],
	index: number,
	markdown: string,
): Segment[] => {
	const trimmed = markdown.replace(/\s+$/, '');
	let text = '';
	if (trimmed) {
		const followedBySection = segments[index + 1]?.kind === 'section';
		text = `${trimmed}\n${followedBySection ? '\n' : ''}`;
	}
	return segments.map((segment, i) =>
		i === index ? { kind: 'free', text } : segment,
	);
};

export type OwnerKind =
	| 'system'
	| 'agents'
	| 'persona'
	| 'plugin'
	| 'user'
	| 'unknown';

export const parseOwner = (
	owner: string,
): { kind: OwnerKind; ref: string | null } => {
	if (owner === 'system' || owner === 'agents')
		return { kind: owner, ref: null };
	const match = /^(persona|plugin|user):(.+)$/.exec(owner);
	if (match) return { kind: match[1] as OwnerKind, ref: match[2] };
	return { kind: 'unknown', ref: null };
};
