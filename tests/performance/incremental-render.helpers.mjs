export const trackNodes = (page, selector) =>
  page.evaluate((sel) => {
    const nodes = [...document.querySelectorAll(sel)];
    nodes.forEach((node) => (node.__tracked = true));
    window.__removed = 0;
    new MutationObserver((records) => {
      for (const record of records)
        for (const node of record.removedNodes)
          if (
            node.__tracked ||
            [...(node.querySelectorAll?.('*') || [])].some((n) => n.__tracked)
          )
            window.__removed++;
    }).observe(document.body, { childList: true, subtree: true });
    return nodes.length;
  }, selector);

export const survivors = (page, selector) =>
  page.evaluate(
    (sel) => ({
      kept: [...document.querySelectorAll(sel)].filter((n) => n.__tracked)
        .length,
      total: document.querySelectorAll(sel).length,
      removed: window.__removed,
    }),
    selector,
  );

export const countLifecycle = (page) =>
  page.addInitScript(() => {
    const counts = {};
    window.__lifecycle = counts;
    window.__VUE_DEVTOOLS_GLOBAL_HOOK__ = {
      enabled: true,
      apps: [],
      appRecords: [],
      on() {},
      once() {},
      off() {},
      emit(event, ...args) {
        const match = /^component:(added|updated|removed)$/.exec(event);
        if (!match) return;
        const instance = args[3] ?? args[2];
        const name = instance?.type?.name || instance?.type?.__name || 'anon';
        counts[name] ||= { added: 0, updated: 0, removed: 0 };
        counts[name][match[1]]++;
      },
    };
  });

export const resetLifecycle = (page) =>
  page.evaluate(() => {
    for (const name in window.__lifecycle)
      window.__lifecycle[name] = { added: 0, updated: 0, removed: 0 };
  });

export const lifecycleOf = (page, name) =>
  page.evaluate(
    (key) => window.__lifecycle[key] || { added: 0, updated: 0, removed: 0 },
    name,
  );
