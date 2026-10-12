import {
  autoUpdate,
  computePosition,
  flip,
  offset,
  shift,
} from '@floating-ui/dom';
import { isSheetViewport, prefersReducedMotion } from '../a11y';
import type { OverlayAdapter, OverlayShowOptions } from './OverlayAdapter';
import { clampIntoViewport } from './clamp';
import { createSheetRoom } from './sheetRoom';

const RING_PADDING = 8;
const RING_RADIUS = 12;

export function createPassiveAdapter(): OverlayAdapter {
  let cleanup: (() => void) | null = null;
  let update: (() => void) | null = null;
  const room = createSheetRoom();

  function hide() {
    room.release();
    cleanup?.();
    cleanup = null;
    update = null;
  }

  function placeRing(ring: HTMLElement, anchor: Element) {
    const r = anchor.getBoundingClientRect();
    Object.assign(ring.style, {
      left: `${r.left - RING_PADDING}px`,
      top: `${r.top - RING_PADDING}px`,
      width: `${r.width + RING_PADDING * 2}px`,
      height: `${r.height + RING_PADDING * 2}px`,
      borderRadius: `${RING_RADIUS}px`,
    });
  }

  function scrollIntoView(anchor: Element, sheet: boolean) {
    const r = anchor.getBoundingClientRect();
    const behavior = prefersReducedMotion() ? 'auto' : 'smooth';
    if (sheet) {
      if (r.bottom > window.innerHeight * 0.5 || r.top < 0) {
        anchor.scrollIntoView({ block: 'center', behavior });
      }
    } else {
      anchor.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior });
    }
  }

  function show({ anchorEl, mode, mount, blocking = true }: OverlayShowOptions) {
    hide();
    const centered = mode === 'centered' || !anchorEl;
    const sheet = isSheetViewport();
    const nodes: HTMLElement[] = [];

    const host = document.createElement('div');
    host.className = 'tour-host tour-host--passive';
    host.dataset.tourHost = 'passive';

    let stopAuto: (() => void) | null = null;

    if (centered) {
      if (blocking) {
        const backdrop = document.createElement('div');
        backdrop.className = 'tour-backdrop';
        document.body.appendChild(backdrop);
        nodes.push(backdrop);
        host.classList.add('tour-host--centered');
      } else {
        host.classList.add(sheet ? 'tour-host--sheet' : 'tour-host--floating');
      }
    } else if (anchorEl) {
      const ring = document.createElement('div');
      ring.className = 'tour-ring';
      ring.setAttribute('aria-hidden', 'true');
      document.body.appendChild(ring);
      nodes.push(ring);
      scrollIntoView(anchorEl, sheet);
      room.watch(anchorEl, sheet, undefined, () => host);

      if (sheet) {
        host.classList.add('tour-host--sheet');
        update = () => placeRing(ring, anchorEl);
        update();
        const onChange = () => update?.();
        window.addEventListener('resize', onChange);
        window.addEventListener('scroll', onChange, true);
        stopAuto = () => {
          window.removeEventListener('resize', onChange);
          window.removeEventListener('scroll', onChange, true);
        };
      } else {
        update = () => {
          placeRing(ring, anchorEl);
          void computePosition(anchorEl, host, {
            strategy: 'fixed',
            placement: 'bottom-start',
            middleware: [
              offset(RING_PADDING + 8),
              flip({ padding: 8 }),
              shift({ padding: 8, crossAxis: true }),
            ],
          }).then(({ x, y }) => {
            host.style.left = `${x}px`;
            host.style.top = `${y}px`;
            clampIntoViewport(host, undefined, anchorEl);
          });
        };
        stopAuto = autoUpdate(anchorEl, host, () => update?.(), {
          animationFrame: true,
        });
      }
    }

    document.body.appendChild(host);
    nodes.push(host);
    const unmount = mount(host);

    const clamp = () => {
      clampIntoViewport(host, undefined, anchorEl);
      if (sheet) room.revealAvoided(host, anchorEl);
    };
    const resize =
      typeof ResizeObserver === 'function' ? new ResizeObserver(clamp) : null;
    resize?.observe(host);
    window.addEventListener('resize', clamp);
    clamp();

    cleanup = () => {
      stopAuto?.();
      resize?.disconnect();
      window.removeEventListener('resize', clamp);
      unmount();
      nodes.forEach((n) => n.remove());
    };
  }

  return {
    show,
    refresh() {
      update?.();
    },
    hide,
  };
}
