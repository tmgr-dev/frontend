const RESCROLL_DELAYS_MS = [350, 800, 1500];

export function isAnchorClear(anchor: Element, sheet: boolean): boolean {
  const r = anchor.getBoundingClientRect();
  const limit = sheet ? window.innerHeight * 0.5 : window.innerHeight;
  return r.top >= 0 && r.bottom <= limit;
}

export function createSheetRoom() {
  let timers: ReturnType<typeof setTimeout>[] = [];
  let padded: Array<[HTMLElement, string]> = [];

  function makeRoom(anchor: Element) {
    for (let el = anchor.parentElement; el && el !== document.body; el = el.parentElement) {
      const style = getComputedStyle(el);
      const scrolls = style.overflowY === 'auto' || style.overflowY === 'scroll';
      const bounded = el.scrollHeight > el.clientHeight || style.maxHeight !== 'none';
      if (!scrolls || !bounded) continue;
      const scroller = el as HTMLElement;
      if (!padded.some(([done]) => done === scroller)) {
        padded.push([scroller, scroller.style.paddingBottom]);
        scroller.style.paddingBottom = `${Math.round(window.innerHeight * 0.5)}px`;
      }
      return;
    }
  }

  return {
    watch(anchor: Element, sheet: boolean, onScrolled?: () => void) {
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
      timers.forEach(clearTimeout);
      timers = [];
      padded.forEach(([el, padding]) => {
        el.style.paddingBottom = padding;
      });
      padded = [];
    },
  };
}
