const RESCROLL_DELAYS_MS = [350, 800, 1500];

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

  function makeRoom(anchor: Element) {
    let el = anchor.parentElement;
    while (el && el !== document.body) {
      const style = getComputedStyle(el);
      const scrolls = style.overflowY === 'auto' || style.overflowY === 'scroll';
      const bounded = el.scrollHeight > el.clientHeight || style.maxHeight !== 'none';
      if (scrolls && bounded) {
        const scroller = el;
        if (!padded.some(([done]) => done === scroller)) {
          padded.push([scroller, scroller.style.paddingBottom]);
          scroller.style.paddingBottom = `${Math.round(window.innerHeight * 0.5)}px`;
        }
        return;
      }
      el = el.parentElement;
    }
  }

  return {
    watch(anchor: Element, sheet: boolean, onScrolled?: () => void) {
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
          if (!anchor.isConnected || isAnchorClear(anchor, sheet)) return;
          if (sheet) makeRoom(anchor);
          anchor.scrollIntoView({ block: 'center', behavior: 'auto' });
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
