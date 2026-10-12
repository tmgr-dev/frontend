const GRACE_MS = 4000;

function isVisible(el: Element): boolean {
  const rect = el.getBoundingClientRect();
  if (rect.width === 0 || rect.height === 0) return false;
  return getComputedStyle(el).display !== 'none';
}

export function findAnchorEl(name: string): Element | null {
  const matches = document.querySelectorAll(
    `[data-tour="${CSS.escape(name)}"]`,
  );
  return Array.from(matches).find(isVisible) ?? null;
}

export function watchAnchorEl(
  name: string | undefined,
  onChange: (el: Element | null) => void,
  graceMs = GRACE_MS,
): () => void {
  if (!name) {
    onChange(null);
    return () => {};
  }
  let current: Element | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let raf = 0;
  let stopped = false;

  let reportedMissing = false;

  const check = () => {
    if (stopped) return;
    const el = findAnchorEl(name);
    if (el) {
      if (timer) clearTimeout(timer);
      timer = null;
      reportedMissing = false;
      if (el !== current) {
        current = el;
        onChange(el);
      }
    } else if (!timer && !reportedMissing) {
      timer = setTimeout(() => {
        timer = null;
        if (stopped || findAnchorEl(name)) return;
        reportedMissing = true;
        current = null;
        onChange(null);
      }, graceMs);
    }
  };

  const observer = new MutationObserver(() => {
    cancelAnimationFrame(raf);
    raf = requestAnimationFrame(check);
  });
  observer.observe(document.body, {
    childList: true,
    subtree: true,
    attributes: true,
  });
  check();

  return () => {
    stopped = true;
    observer.disconnect();
    cancelAnimationFrame(raf);
    if (timer) clearTimeout(timer);
  };
}
