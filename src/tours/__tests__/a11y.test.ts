/** @jest-environment jsdom */
import {
  announce,
  focusAnchor,
  inertAround,
  isKeyboardOpen,
  isTransientLayerOpen,
  onEscape,
  rememberFocus,
  removeLiveRegion,
  stepAnnouncement,
  trapFocus,
} from '../a11y';

const tab = (shiftKey = false) =>
  new KeyboardEvent('keydown', {
    key: 'Tab',
    shiftKey,
    bubbles: true,
    cancelable: true,
  });

beforeEach(() => {
  document.body.innerHTML = '';
});

describe('stepAnnouncement', () => {
  it('adds the waiting suffix only for wait steps', () => {
    expect(
      stepAnnouncement({
        position: 2,
        total: 3,
        title: 'Start the timer',
        waiting: true,
      }),
    ).toBe('Step 2 of 3: Start the timer. Waiting for you.');
    expect(
      stepAnnouncement({ position: 1, total: 3, title: 'Hello', waiting: false }),
    ).toBe('Step 1 of 3: Hello.');
  });
});

describe('announce', () => {
  it('writes into a single polite live region', () => {
    announce('one');
    announce('two');
    const regions = document.querySelectorAll('[aria-live="polite"]');
    expect(regions).toHaveLength(1);
    expect(regions[0].textContent).toBe('two');
    expect(regions[0].getAttribute('role')).toBe('status');
    removeLiveRegion();
    expect(document.querySelectorAll('[aria-live]')).toHaveLength(0);
  });
});

describe('trapFocus', () => {
  function setup() {
    document.body.innerHTML = `
      <button id="outside">outside</button>
      <div id="box"><button id="a">a</button><button id="b">b</button></div>`;
    return {
      box: document.getElementById('box') as HTMLElement,
      a: document.getElementById('a') as HTMLElement,
      b: document.getElementById('b') as HTMLElement,
      outside: document.getElementById('outside') as HTMLElement,
    };
  }

  it('focuses the container, wraps Tab and Shift+Tab', () => {
    const { box, a, b } = setup();
    const release = trapFocus(box);
    expect(document.activeElement).toBe(box);
    const e1 = tab();
    b.focus();
    b.dispatchEvent(e1);
    expect(e1.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(a);
    const e2 = tab(true);
    a.dispatchEvent(e2);
    expect(document.activeElement).toBe(b);
    release();
  });

  it('pulls focus back when it escapes and stops at release', () => {
    const { box, outside } = setup();
    const release = trapFocus(box);
    outside.focus();
    expect(box.contains(document.activeElement)).toBe(true);
    release();
    outside.focus();
    expect(document.activeElement).toBe(outside);
  });

  it('stops Tab from reaching later listeners such as driver.js', () => {
    const { box, a } = setup();
    const release = trapFocus(box);
    const later = jest.fn();
    window.addEventListener('keydown', later);
    a.focus();
    a.dispatchEvent(tab());
    expect(later).not.toHaveBeenCalled();
    window.removeEventListener('keydown', later);
    release();
  });
});

describe('inertAround', () => {
  it('inerts everything outside the kept paths, never an ancestor of the anchor', () => {
    document.body.innerHTML = `
      <div id="app"><header id="head"></header><main id="main"><button id="anchor"></button><p id="sibling"></p></main></div>
      <div role="dialog" id="dlg"></div>
      <div id="host"></div>`;
    const $ = (id: string) => document.getElementById(id) as HTMLElement;
    const restore = inertAround([$('host'), $('anchor')]);
    expect($('app').hasAttribute('inert')).toBe(false);
    expect($('main').hasAttribute('inert')).toBe(false);
    expect($('anchor').hasAttribute('inert')).toBe(false);
    expect($('head').hasAttribute('inert')).toBe(true);
    expect($('sibling').hasAttribute('inert')).toBe(true);
    expect($('dlg').hasAttribute('inert')).toBe(true);
    expect($('host').hasAttribute('inert')).toBe(false);
    restore();
    expect(document.querySelectorAll('[inert]')).toHaveLength(0);
  });

  it('inerts the whole app root when there is no anchor and keeps live region audible', () => {
    document.body.innerHTML = `<div id="app"></div><div id="host"></div>`;
    announce('x');
    const restore = inertAround([document.getElementById('host')]);
    expect(document.getElementById('app')?.hasAttribute('inert')).toBe(true);
    expect(document.getElementById('tmgr-tour-live')?.hasAttribute('inert')).toBe(
      false,
    );
    restore();
  });

  it('does not restore elements that were already inert', () => {
    document.body.innerHTML = `<div id="x" inert></div><div id="host"></div>`;
    inertAround([document.getElementById('host')])();
    expect(document.getElementById('x')?.hasAttribute('inert')).toBe(true);
  });
});

describe('onEscape', () => {
  const esc = () =>
    new KeyboardEvent('keydown', {
      key: 'Escape',
      bubbles: true,
      cancelable: true,
    });

  it('pauses and swallows the event before other handlers see it', () => {
    const handler = jest.fn();
    const other = jest.fn();
    document.addEventListener('keydown', other, true);
    const off = onEscape(handler);
    const e = esc();
    document.body.dispatchEvent(e);
    expect(handler).toHaveBeenCalledTimes(1);
    expect(other).not.toHaveBeenCalled();
    expect(e.defaultPrevented).toBe(true);
    off();
    document.body.dispatchEvent(esc());
    expect(handler).toHaveBeenCalledTimes(1);
    document.removeEventListener('keydown', other, true);
  });

  it('lets Esc close an open menu or listbox instead of pausing', () => {
    document.body.innerHTML = '<div role="menu" data-state="open"></div>';
    expect(isTransientLayerOpen()).toBe(true);
    const handler = jest.fn();
    const off = onEscape(handler);
    document.body.dispatchEvent(esc());
    expect(handler).not.toHaveBeenCalled();
    off();
  });
});

describe('onEscape scoping', () => {
  const esc = () =>
    new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });

  it('ignores Esc raised inside a foreign dialog', () => {
    document.body.innerHTML =
      '<div role="dialog" id="foreign"><input id="f" /></div><div role="dialog" id="own"><button id="o"></button></div>';
    const handler = jest.fn();
    const other = jest.fn();
    document.addEventListener('keydown', other, true);
    const off = onEscape(handler, {
      ownDialog: () => document.getElementById('own'),
    });
    const e = esc();
    document.getElementById('f')?.dispatchEvent(e);
    expect(handler).not.toHaveBeenCalled();
    expect(other).toHaveBeenCalled();
    expect(e.defaultPrevented).toBe(false);
    document.getElementById('o')?.dispatchEvent(esc());
    expect(handler).toHaveBeenCalledTimes(1);
    off();
    document.removeEventListener('keydown', other, true);
  });

  it('does not stop propagation in passive mode but still pauses', () => {
    document.body.innerHTML = '<input id="i" />';
    const handler = jest.fn();
    const other = jest.fn();
    document.addEventListener('keydown', other, true);
    const off = onEscape(handler, { swallow: () => false });
    const e = esc();
    document.getElementById('i')?.dispatchEvent(e);
    expect(handler).toHaveBeenCalledTimes(1);
    expect(other).toHaveBeenCalled();
    expect(e.defaultPrevented).toBe(false);
    off();
    document.removeEventListener('keydown', other, true);
  });
});

describe('focus helpers', () => {
  it('restores focus to the remembered element', () => {
    document.body.innerHTML = '<button id="a"></button><button id="b"></button>';
    const a = document.getElementById('a') as HTMLElement;
    a.focus();
    const restore = rememberFocus();
    document.getElementById('b')?.focus();
    restore();
    expect(document.activeElement).toBe(a);
  });

  it('skips restore when the element left the DOM', () => {
    document.body.innerHTML = '<button id="a"></button>';
    (document.getElementById('a') as HTMLElement).focus();
    const restore = rememberFocus();
    document.body.innerHTML = '';
    expect(() => restore()).not.toThrow();
  });

  it('falls back to the page heading when the remembered element is gone', () => {
    document.body.innerHTML = '<h1 id="h">Page</h1><button id="a"></button>';
    (document.getElementById('a') as HTMLElement).focus();
    const restore = rememberFocus();
    document.getElementById('a')?.remove();
    restore();
    expect(document.activeElement?.id).toBe('h');
  });

  it('focuses the anchor or its first focusable descendant', () => {
    document.body.innerHTML =
      '<button id="a"></button><div id="wrap"><span></span><input id="in"></div><div id="none"></div>';
    expect(focusAnchor(document.getElementById('a'))).toBe(true);
    expect(document.activeElement?.id).toBe('a');
    expect(focusAnchor(document.getElementById('wrap'))).toBe(true);
    expect(document.activeElement?.id).toBe('in');
    expect(focusAnchor(document.getElementById('none'))).toBe(false);
    expect(focusAnchor(null)).toBe(false);
  });
});

describe('isKeyboardOpen', () => {
  it('detects a shrunk visual viewport', () => {
    expect(isKeyboardOpen(844, 844)).toBe(false);
    expect(isKeyboardOpen(520, 844)).toBe(true);
    expect(isKeyboardOpen(0, 0)).toBe(false);
  });
});
