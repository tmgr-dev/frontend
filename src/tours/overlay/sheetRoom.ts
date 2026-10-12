import { AVOID_ATTR } from './avoid';

const RESCROLL_DELAYS_MS = [350, 800, 1500];
const AVOID_GAP = 12;
const ANCHOR_KEEP = 48;

export function isAnchorClear(anchor: Element, sheet: boolean): boolean {
  const r = anchor.getBoundingClientRect();
  const limit = sheet ? window.innerHeight * 0.5 : window.innerHeight;
  if (r.height > limit) return r.top >= 0 && r.top < limit;
  return r.top >= 0 && r.bottom <= limit;
}

const USER_INPUT_EVENTS = ['wheel', 'touchstart', 'touchmove'] as const;

export function createSheetRoom() {
  let timers: ReturnType<typeof setTimeout>[] = [];
  let padded: Array<[HTMLElement, string]> = [];
  let stopInput: (() => void) | null = null;

  function findScroller(from: Element, requireBounded = true): HTMLElement | null {
    let el = from.parentElement;
    while (el && el !== document.body) {
      const style = getComputedStyle(el);
      const scrolls = style.overflowY === 'auto' || style.overflowY === 'scroll';
      const bounded = el.scrollHeight > el.clientHeight || style.maxHeight !== 'none';
      if (scrolls && (bounded || !requireBounded)) return el;
      el = el.parentElement;
    }
    return null;
  }

  function padScroller(scroller: HTMLElement, px: number) {
    if (!padded.some(([done]) => done === scroller)) {
      padded.push([scroller, scroller.style.paddingBottom]);
    }
    const current = parseFloat(getComputedStyle(scroller).paddingBottom) || 0;
    if (current < px) scroller.style.paddingBottom = `${Math.round(px)}px`;
  }

  function makeRoom(anchor: Element) {
    const scroller = findScroller(anchor);
    if (scroller) padScroller(scroller, window.innerHeight * 0.5);
  }

  function revealAvoided(sheet: HTMLElement | null, anchor: Element | null) {
    if (!sheet) return;
    const sheetRect = sheet.getBoundingClientRect();
    if (sheetRect.height <= 0) return;
    const limit = sheetRect.top - AVOID_GAP;
    const targets = [...document.querySelectorAll<HTMLElement>(`[${AVOID_ATTR}]`)].filter(
      (el) => !(anchor && (el === anchor || el.contains(anchor) || anchor.contains(el))),
    );
    for (const target of targets) {
      const r = target.getBoundingClientRect();
      if (r.width <= 0 || r.height <= 0 || r.bottom <= limit) continue;
      let delta = r.bottom - limit;
      const anchorRect = anchor?.getBoundingClientRect();
      if (anchorRect) delta = Math.min(delta, Math.max(0, anchorRect.bottom - ANCHOR_KEEP));
      if (delta <= 0) continue;
      const scroller = findScroller(target, false);
      if (scroller) {
        padScroller(scroller, sheetRect.height + AVOID_GAP * 2);
        scroller.scrollTop += delta;
      } else {
        window.scrollBy(0, delta);
      }
    }
  }

  return {
    revealAvoided,
    watch(
      anchor: Element,
      sheet: boolean,
      onScrolled?: () => void,
      getSheet?: () => HTMLElement | null,
    ) {
      stopInput?.();
      const cancel = () => {
        timers.forEach(clearTimeout);
        timers = [];
        stopInput?.();
      };
      USER_INPUT_EVENTS.forEach((type) =>
        window.addEventListener(type, cancel, { passive: true, once: true }),
      );
      stopInput = () => {
        USER_INPUT_EVENTS.forEach((type) => window.removeEventListener(type, cancel));
        stopInput = null;
      };
      timers = RESCROLL_DELAYS_MS.map((delay) =>
        setTimeout(() => {
          if (!anchor.isConnected) return;
          if (!isAnchorClear(anchor, sheet)) {
            if (sheet) makeRoom(anchor);
            anchor.scrollIntoView({ block: 'center', behavior: 'auto' });
          }
          if (sheet && getSheet) revealAvoided(getSheet(), anchor);
          onScrolled?.();
        }, delay),
      );
    },
    release() {
      stopInput?.();
      timers.forEach(clearTimeout);
      timers = [];
      padded.forEach(([el, padding]) => {
        el.style.paddingBottom = padding;
      });
      padded = [];
    },
  };
}
