export interface RectLike {
  left: number;
  top: number;
  right: number;
  bottom: number;
  width: number;
  height: number;
}

export const VIEWPORT_PADDING = 8;

const axis = (start: number, size: number, limit: number, pad: number): number => {
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

export function clampIntoViewport(el: HTMLElement, pad = VIEWPORT_PADDING): void {
  el.style.translate = '';
  if (el.classList.contains('tour-host--sheet')) pad = 0;
  const { dx, dy } = clampOffset(
    el.getBoundingClientRect(),
    { width: window.innerWidth, height: window.innerHeight },
    pad,
  );
  if (dx || dy) el.style.translate = `${dx}px ${dy}px`;
}
