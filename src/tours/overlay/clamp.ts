import { pickAvoidingPosition, visibleAvoidRects } from './avoid';

export interface RectLike {
	left: number;
	top: number;
	right: number;
	bottom: number;
	width: number;
	height: number;
}

export const VIEWPORT_PADDING = 8;

const axis = (
	start: number,
	size: number,
	limit: number,
	pad: number,
): number => {
	if (size > limit - pad * 2 || start < pad) return pad - start;
	if (start + size > limit - pad) return limit - pad - (start + size);
	return 0;
};

export function clampOffset(
	rect: RectLike,
	viewport: { width: number; height: number },
	pad = VIEWPORT_PADDING,
): { dx: number; dy: number } {
	return {
		dx: axis(rect.left, rect.width, viewport.width, pad),
		dy: axis(rect.top, rect.height, viewport.height, pad),
	};
}

export function clampIntoViewport(
	el: HTMLElement,
	pad = VIEWPORT_PADDING,
	anchor: Element | null = null,
): void {
	el.style.translate = '';
	if (el.classList.contains('tour-host--sheet')) return;
	const viewport = { width: window.innerWidth, height: window.innerHeight };
	const rect = el.getBoundingClientRect();
	const { dx, dy } = clampOffset(rect, viewport, pad);
	let tx = dx;
	let ty = dy;
	const avoid = visibleAvoidRects(anchor);
	if (avoid.length) {
		const moved = {
			left: rect.left + dx,
			top: rect.top + dy,
			right: rect.right + dx,
			bottom: rect.bottom + dy,
			width: rect.width,
			height: rect.height,
		};
		const spot = pickAvoidingPosition({
			card: moved,
			anchor: anchor ? anchor.getBoundingClientRect() : null,
			avoid,
			viewport,
			pad,
		});
		if (spot) {
			tx = spot.left - rect.left;
			ty = spot.top - rect.top;
		}
	}
	if (tx || ty) el.style.translate = `${tx}px ${ty}px`;
}

export function createFrameClamp(
	find: () => HTMLElement | null,
	anchor: () => Element | null = () => null,
) {
	let frame = 0;
	const run = () => {
		frame = 0;
		const el = find();
		if (el && !el.classList.contains('tour-sheet-mode'))
			clampIntoViewport(el, VIEWPORT_PADDING, anchor());
	};
	const schedule = () => {
		if (frame) cancelAnimationFrame(frame);
		frame = requestAnimationFrame(run);
	};
	const stop = () => {
		if (frame) cancelAnimationFrame(frame);
		frame = 0;
		window.removeEventListener('scroll', schedule, true);
		window.removeEventListener('resize', schedule);
	};
	const start = () => {
		stop();
		window.addEventListener('scroll', schedule, true);
		window.addEventListener('resize', schedule);
		schedule();
	};
	return { schedule, start, stop };
}
