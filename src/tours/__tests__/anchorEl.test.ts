/** @jest-environment jsdom */
import { findAnchorEl, watchAnchorEl } from '../overlay/anchorEl';

const rect = { width: 10, height: 10 } as DOMRect;

function addAnchor(name: string, visible = true) {
  const el = document.createElement('button');
  el.setAttribute('data-tour', name);
  el.getBoundingClientRect = () =>
    visible ? rect : ({ width: 0, height: 0 } as DOMRect);
  document.body.appendChild(el);
  return el;
}

beforeAll(() => {
  (globalThis as { CSS?: unknown }).CSS = { escape: (s: string) => s };
});

beforeEach(() => {
  document.body.innerHTML = '';
  jest.useFakeTimers();
});

afterEach(() => jest.useRealTimers());

describe('findAnchorEl', () => {
  it('returns the first visible match and skips zero-size ones', () => {
    addAnchor('nav.menu-button', false);
    const visible = addAnchor('nav.menu-button');
    expect(findAnchorEl('nav.menu-button')).toBe(visible);
    expect(findAnchorEl('missing')).toBeNull();
  });
});

describe('watchAnchorEl', () => {
  it('reports an existing anchor immediately', () => {
    const el = addAnchor('a');
    const cb = jest.fn();
    const stop = watchAnchorEl('a', cb);
    expect(cb).toHaveBeenCalledWith(el);
    stop();
  });

  it('reports null once the grace period passes without an anchor', () => {
    const cb = jest.fn();
    const stop = watchAnchorEl('a', cb, 4000);
    jest.advanceTimersByTime(3999);
    expect(cb).not.toHaveBeenCalled();
    jest.advanceTimersByTime(2);
    expect(cb).toHaveBeenCalledWith(null);
    stop();
  });

  it('reports a late anchor and cancels the pending null', async () => {
    const cb = jest.fn();
    const stop = watchAnchorEl('late', cb, 4000);
    const el = addAnchor('late');
    await Promise.resolve();
    jest.advanceTimersByTime(20);
    expect(cb).toHaveBeenCalledWith(el);
    jest.advanceTimersByTime(5000);
    expect(cb).not.toHaveBeenCalledWith(null);
    stop();
  });

  it('reports null immediately when no anchor name is given', () => {
    const cb = jest.fn();
    watchAnchorEl(undefined, cb);
    expect(cb).toHaveBeenCalledWith(null);
  });
});
