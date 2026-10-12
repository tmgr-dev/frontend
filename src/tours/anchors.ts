import type { StepDef } from './types';

export const ANCHOR_TIMEOUT_MS = 4000;
const POLL_MS = 250;

export function isVisible(el: Element): boolean {
  const rect = el.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return false;
  const style = getComputedStyle(el);
  return style.display !== 'none' && style.visibility !== 'hidden';
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
  const find = () => Array.from(root.querySelectorAll(selector)).find(isVisible) ?? null;

  return new Promise((resolve) => {
    const first = find();
    if (first) return resolve(first);

    let observer: MutationObserver | null = null;
    let poll: ReturnType<typeof setInterval> | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const finish = (el: Element | null) => {
      observer?.disconnect();
      clearInterval(poll);
      clearTimeout(timer);
      options.signal?.removeEventListener('abort', onAbort);
      resolve(el);
    };
    const check = () => {
      const el = find();
      if (el) finish(el);
    };
    const onAbort = () => finish(null);

    if (options.signal?.aborted) return resolve(null);
    options.signal?.addEventListener('abort', onAbort);
    observer = new MutationObserver(check);
    observer.observe(root.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
    });
    poll = setInterval(check, POLL_MS);
    timer = setTimeout(() => finish(null), options.timeoutMs ?? ANCHOR_TIMEOUT_MS);
  });
}
