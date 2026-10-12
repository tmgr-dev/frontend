import type { StepDef } from './types';

export const ANCHOR_TIMEOUT_MS = 4000;
const POLL_MS = 250;

export function isVisible(el: Element): boolean {
  const rect = el.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return false;
  const style = getComputedStyle(el);
  return style.display !== 'none' && style.visibility !== 'hidden';
}

export const OWN_LAYERS = '.driver-overlay, .driver-popover, .tour-backdrop, .tour-host, .tour-ring';

const IGNORED_HITS = `${OWN_LAYERS}, [data-sidebar="rail"]`;

export function isOccluded(el: Element): boolean {
  if (typeof document.elementFromPoint !== 'function') return false;
  const rect = el.getBoundingClientRect();
  const inset = Math.min(8, rect.width / 4, rect.height / 4);
  const left = rect.left + inset;
  const right = rect.right - inset;
  const top = rect.top + inset;
  const bottom = rect.bottom - inset;
  const points: Array<[number, number]> = [
    [(left + right) / 2, (top + bottom) / 2],
    [right, bottom],
  ];
  return points.some(([x, y]) => {
    if (x < 0 || y < 0 || x >= window.innerWidth || y >= window.innerHeight) return false;
    const hit = document.elementFromPoint(x, y);
    if (!hit || hit.closest(IGNORED_HITS)) return false;
    return !(el.contains(hit) || hit.contains(el));
  });
}

export function pickAnchorName(step: StepDef, mobile: boolean): string | undefined {
  return mobile && step.mobileAnchor ? step.mobileAnchor : step.anchor;
}

export function anchorSelector(name: string, taskId?: number | null): string {
  const base = `[data-tour="${name}"]`;
  if (taskId == null) return base;
  const task = `[data-task-id="${taskId}"]`;
  return `${base}${task}, ${task} ${base}, ${base} ${task}`;
}

export interface ResolveOptions {
  mobile: boolean;
  eventTaskId?: number | null;
  timeoutMs?: number;
  signal?: AbortSignal;
  root?: Document;
}

export function resolveAnchor(step: StepDef, options: ResolveOptions): Promise<Element | null> {
  const name = pickAnchorName(step, options.mobile);
  if (!name) return Promise.resolve(null);
  const root = options.root ?? document;
  const selector = anchorSelector(
    name,
    step.anchorScope === 'eventTask' ? options.eventTaskId : null,
  );
  const find = () => Array.from(root.querySelectorAll(selector)).find((el) => isVisible(el) && !isOccluded(el)) ?? null;

  return new Promise((resolve) => {
    const first = find();
    if (first) return resolve(first);

    let observer: MutationObserver | null = null;
    let poll: ReturnType<typeof setInterval> | undefined;
    let frame: number | undefined;
    let frameTimer: ReturnType<typeof setTimeout> | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const finish = (el: Element | null) => {
      observer?.disconnect();
      clearInterval(poll);
      clearTimeout(timer);
      clearTimeout(frameTimer);
      if (frame !== undefined) cancelAnimationFrame(frame);
      options.signal?.removeEventListener('abort', onAbort);
      resolve(el);
    };
    const check = () => {
      const el = find();
      if (el) finish(el);
    };
    const onAbort = () => finish(null);
    const schedule = () => {
      if (frame !== undefined || frameTimer !== undefined) return;
      const run = () => {
        frame = undefined;
        frameTimer = undefined;
        check();
      };
      if (typeof requestAnimationFrame === 'function') frame = requestAnimationFrame(run);
      else frameTimer = setTimeout(run, 16);
    };

    if (options.signal?.aborted) return resolve(null);
    options.signal?.addEventListener('abort', onAbort);
    observer = new MutationObserver(schedule);
    observer.observe(root.documentElement, { childList: true, subtree: true });
    poll = setInterval(check, POLL_MS);
    timer = setTimeout(() => finish(null), options.timeoutMs ?? ANCHOR_TIMEOUT_MS);
  });
}
