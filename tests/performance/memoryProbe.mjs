export const instrument = (page) =>
  page.addInitScript(() => {
    const live = new Map();
    const bump = (key, d) => live.set(key, (live.get(key) ?? 0) + d);
    const name = (t) =>
      t === window
        ? 'window'
        : t === document
        ? 'document'
        : t?.constructor?.name ?? 'other';
    const listeners = new WeakMap();
    const sig = (t, type, fn, opts) => {
      const cap = typeof opts === 'object' ? !!opts?.capture : !!opts;
      return `${type}|${cap}`;
    };
    const add = EventTarget.prototype.addEventListener;
    const remove = EventTarget.prototype.removeEventListener;
    EventTarget.prototype.addEventListener = function (type, fn, opts) {
      if (
        fn &&
        (this === window ||
          this === document ||
          this === document.body ||
          this === document.documentElement)
      ) {
        let set = listeners.get(this);
        if (!set) listeners.set(this, (set = new Map()));
        const k = sig(this, type, fn, opts);
        const fns = set.get(k) ?? new Set();
        if (!fns.has(fn)) {
          fns.add(fn);
          set.set(k, fns);
          bump(`listener:${name(this)}:${type}`, 1);
        }
      }
      return add.call(this, type, fn, opts);
    };
    EventTarget.prototype.removeEventListener = function (type, fn, opts) {
      const set = listeners.get(this);
      const k = sig(this, type, fn, opts);
      if (set?.get(k)?.delete(fn)) {
        bump(`listener:${name(this)}:${type}`, -1);
      }
      return remove.call(this, type, fn, opts);
    };
    const si = window.setInterval;
    const ci = window.clearInterval;
    const ids = new Set();
    window.setInterval = function (...a) {
      const id = si.apply(this, a);
      ids.add(id);
      bump('interval', 1);
      return id;
    };
    window.clearInterval = function (id) {
      if (ids.delete(id)) bump('interval', -1);
      return ci.call(this, id);
    };
    const OW = window.Worker;
    if (OW) {
      window.Worker = class extends OW {
        constructor(...a) {
          super(...a);
          bump('worker', 1);
        }
        terminate() {
          bump('worker', -1);
          return super.terminate();
        }
      };
    }
    const OE = window.EventSource;
    if (OE) {
      window.EventSource = class extends OE {
        constructor(...a) {
          super(...a);
          bump('eventsource', 1);
        }
        close() {
          bump('eventsource', -1);
          return super.close();
        }
      };
    }
    window.__live = () =>
      Object.fromEntries([...live].filter(([, v]) => v !== 0));
  });

export const recordSockets = (page, log) =>
  page.routeWebSocket(/.*/, (ws) => {
    ws.onMessage((m) => {
      try {
        const msg = JSON.parse(m);
        if (msg.event === 'pusher:subscribe') log.push(`+${msg.data.channel}`);
        if (msg.event === 'pusher:unsubscribe')
          log.push(`-${msg.data.channel}`);
      } catch {}
    });
    ws.send(
      JSON.stringify({
        event: 'pusher:connection_established',
        data: JSON.stringify({ socket_id: '1.1', activity_timeout: 120 }),
      }),
    );
  });

export const heap = async (page) => {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('HeapProfiler.enable');
  await cdp.send('HeapProfiler.collectGarbage');
  await cdp.send('HeapProfiler.collectGarbage');
  const { metrics } = await cdp
    .send('Performance.getMetrics')
    .catch(() => ({ metrics: [] }));
  await cdp.detach();
  return metrics;
};
