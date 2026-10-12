const FOCUSABLE =
  'a[href],button:not([disabled]),input:not([disabled]):not([type="hidden"]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])';

const LIVE_REGION_ID = 'tmgr-tour-live';

export function stepAnnouncement(o: {
  position: number;
  total: number;
  title: string;
  waiting: boolean;
}): string {
  const base = `Step ${o.position} of ${o.total}: ${o.title}.`;
  return o.waiting ? `${base} Waiting for you.` : base;
}

export function announce(text: string): void {
  let region = document.getElementById(LIVE_REGION_ID);
  if (!region) {
    region = document.createElement('div');
    region.id = LIVE_REGION_ID;
    region.setAttribute('role', 'status');
    region.setAttribute('aria-live', 'polite');
    region.setAttribute('aria-atomic', 'true');
    region.style.cssText =
      'position:fixed;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap;';
    document.body.appendChild(region);
  }
  region.textContent = '';
  region.textContent = text;
}

export function removeLiveRegion(): void {
  document.getElementById(LIVE_REGION_ID)?.remove();
}

function isVisible(el: HTMLElement): boolean {
  if (el.hidden) return false;
  const style = getComputedStyle(el);
  return style.display !== 'none' && style.visibility !== 'hidden';
}

export function getFocusable(container: Element): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (el) => isVisible(el) && !el.closest('[inert]'),
  );
}

export function rememberFocus(): () => void {
  const previous = document.activeElement;
  return () => {
    if (
      previous instanceof HTMLElement &&
      previous !== document.body &&
      previous.isConnected
    ) {
      previous.focus({ preventScroll: true });
      return;
    }
    const heading = document.querySelector<HTMLElement>('h1');
    if (heading && previous instanceof HTMLElement && previous !== document.body) {
      heading.tabIndex = -1;
      heading.focus({ preventScroll: true });
    }
  };
}

export function focusAnchor(anchor: Element | null): boolean {
  if (!anchor) return false;
  const target =
    anchor instanceof HTMLElement && anchor.matches(FOCUSABLE)
      ? anchor
      : getFocusable(anchor)[0];
  if (!target) return false;
  target.focus({ preventScroll: true });
  return true;
}

export function trapFocus(
  container: HTMLElement,
  initialFocus?: HTMLElement | null,
): () => void {
  const first = initialFocus ?? container;
  if (first === container && !container.hasAttribute('tabindex')) {
    container.setAttribute('tabindex', '-1');
  }
  first.focus({ preventScroll: true });

  const onKeydown = (e: KeyboardEvent) => {
    if (e.key !== 'Tab') return;
    const items = getFocusable(container);
    e.preventDefault();
    e.stopImmediatePropagation();
    if (items.length === 0) {
      container.focus({ preventScroll: true });
      return;
    }
    const index = items.indexOf(document.activeElement as HTMLElement);
    const step = e.shiftKey ? -1 : 1;
    const next =
      index === -1
        ? e.shiftKey
          ? items.length - 1
          : 0
        : (index + step + items.length) % items.length;
    items[next].focus({ preventScroll: true });
  };

  let pulling = false;
  const onFocusin = (e: FocusEvent) => {
    const target = e.target as Node | null;
    if (pulling || !target || container.contains(target)) return;
    pulling = true;
    try {
      (getFocusable(container)[0] ?? container).focus({ preventScroll: true });
    } finally {
      pulling = false;
    }
  };

  window.addEventListener('keydown', onKeydown, true);
  document.addEventListener('focusin', onFocusin, true);
  return () => {
    window.removeEventListener('keydown', onKeydown, true);
    document.removeEventListener('focusin', onFocusin, true);
  };
}

export function inertAround(keep: Array<Element | null | undefined>): () => void {
  const kept = keep.filter((el): el is Element => !!el);
  const onPath = new Set<Element>();
  kept.forEach((el) => {
    for (let n: Element | null = el; n; n = n.parentElement) onPath.add(n);
  });
  const marked: Element[] = [];
  const visit = (parent: Element) => {
    Array.from(parent.children).forEach((child) => {
      if (kept.includes(child)) return;
      if (onPath.has(child)) {
        visit(child);
        return;
      }
      if (
        child.id === LIVE_REGION_ID ||
        child.hasAttribute('inert') ||
        ['SCRIPT', 'STYLE', 'LINK'].includes(child.tagName)
      ) {
        return;
      }
      child.setAttribute('inert', '');
      marked.push(child);
    });
  };
  visit(document.body);
  return () => marked.forEach((el) => el.removeAttribute('inert'));
}

export function isTransientLayerOpen(doc: Document = document): boolean {
  return !!doc.querySelector(
    '[role="menu"][data-state="open"],[role="listbox"][data-state="open"],[data-tour-menu="open"]',
  );
}

export interface EscapeOptions {
  swallow?: () => boolean;
  ownDialog?: () => Element | null;
}

export function onEscape(handler: () => void, options: EscapeOptions = {}): () => void {
  const listener = (e: KeyboardEvent) => {
    if (e.key !== 'Escape' || e.isComposing) return;
    if (isTransientLayerOpen()) return;
    const target = e.target instanceof Element ? e.target : null;
    const dialog = target?.closest('[role="dialog"]') ?? null;
    if (dialog && dialog !== options.ownDialog?.()) return;
    if (options.swallow?.() ?? true) {
      e.preventDefault();
      e.stopImmediatePropagation();
    }
    handler();
  };
  window.addEventListener('keydown', listener, true);
  return () => window.removeEventListener('keydown', listener, true);
}

export function prefersReducedMotion(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

export const SHEET_QUERY = '(max-width: 639px)';

export function isSheetViewport(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia(SHEET_QUERY).matches
  );
}

export function isKeyboardOpen(
  viewportHeight: number,
  layoutHeight: number,
): boolean {
  return layoutHeight > 0 && viewportHeight < layoutHeight * 0.75;
}
