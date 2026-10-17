import type { RectLike } from './clamp';

export const AVOID_ATTR = 'data-tour-avoid';
const GAP = 12;
const MARGIN = 4;

export interface Point {
	left: number;
	top: number;
}

export const collides = (a: RectLike, b: RectLike, margin = 0): boolean =>
	a.left < b.right + margin &&
	b.left < a.right + margin &&
	a.top < b.bottom + margin &&
	b.top < a.bottom + margin;

const overlapArea = (a: RectLike, b: RectLike): number => {
	const w = Math.min(a.right, b.right) - Math.max(a.left, b.left);
	const h = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
	return w > 0 && h > 0 ? w * h : 0;
};

interface Input {
	card: RectLike;
	anchor: RectLike | null;
	avoid: RectLike[];
	viewport: { width: number; height: number };
	pad?: number;
}

export function pickAvoidingPosition({
	card,
	anchor,
	avoid,
	viewport,
	pad = 8,
}: Input): Point | null {
	if (!avoid.some((r) => collides(card, r, MARGIN))) return null;
	const { width: w, height: h } = card;
	const minLeft = pad;
	const minTop = pad;
	const maxLeft = Math.max(pad, viewport.width - pad - w);
	const maxTop = Math.max(pad, viewport.height - pad - h);
	const fit = (left: number, top: number): Point => ({
		left: Math.min(Math.max(left, minLeft), maxLeft),
		top: Math.min(Math.max(top, minTop), maxTop),
	});

	const candidates: Point[] = [];
	if (anchor) {
		candidates.push(
			fit(anchor.left, anchor.bottom + GAP),
			fit(anchor.left, anchor.top - GAP - h),
			fit(anchor.right + GAP, anchor.top),
			fit(anchor.left - GAP - w, anchor.top),
		);
	}
	candidates.push(
		fit(maxLeft, maxTop),
		fit(minLeft, maxTop),
		fit(maxLeft, minTop),
		fit(minLeft, minTop),
		fit((viewport.width - w) / 2, minTop),
		fit((viewport.width - w) / 2, maxTop),
	);

	const at = (p: Point): RectLike => ({
		left: p.left,
		top: p.top,
		right: p.left + w,
		bottom: p.top + h,
		width: w,
		height: h,
	});
	const clearOfAvoid = (r: RectLike) =>
		!avoid.some((a) => collides(r, a, MARGIN));

	const strict = candidates.find((p) => {
		const r = at(p);
		return clearOfAvoid(r) && (!anchor || !collides(r, anchor));
	});
	if (strict) return strict;
	const loose = candidates.find((p) => clearOfAvoid(at(p)));
	if (loose) return loose;

	const cost = (p: Point) =>
		avoid.reduce((sum, a) => sum + overlapArea(at(p), a), 0);
	return candidates.reduce((best, p) => (cost(p) < cost(best) ? p : best));
}

export function visibleAvoidRects(ignore?: Element | null): RectLike[] {
	if (typeof document === 'undefined') return [];
	return [...document.querySelectorAll<HTMLElement>(`[${AVOID_ATTR}]`)]
		.filter(
			(el) =>
				!(
					ignore &&
					(el === ignore || el.contains(ignore) || ignore.contains(el))
				),
		)
		.map((el) => el.getBoundingClientRect())
		.filter((r) => r.width > 0 && r.height > 0);
}
