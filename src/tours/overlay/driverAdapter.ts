import { isSheetViewport, prefersReducedMotion } from '../a11y';
import type { OverlayAdapter, OverlayShowOptions } from './OverlayAdapter';
import { clampIntoViewport } from './clamp';
import { createSheetRoom } from './sheetRoom';

type DriverModule = typeof import('driver.js');
type Driver = ReturnType<DriverModule['driver']>;

let driverModule: Promise<DriverModule> | null = null;

function loadDriver(): Promise<DriverModule> {
  if (!driverModule) {
    driverModule = Promise.all([
      import('driver.js'),
      import('driver.js/dist/driver.css'),
    ]).then(([mod]) => mod);
  }
  return driverModule;
}

export function createDriverAdapter(): OverlayAdapter {
  let seq = 0;
  let instance: Driver | null = null;
  let unmount: (() => void) | null = null;
  const room = createSheetRoom();

  function hide() {
    seq += 1;
    room.release();
    unmount?.();
    unmount = null;
    instance?.destroy();
    instance = null;
  }

  function clampPopover() {
    const el = document.querySelector<HTMLElement>('.driver-popover.tour-driver-popover');
    if (el && !el.classList.contains('tour-sheet-mode')) clampIntoViewport(el);
  }

  function show({ anchorEl, mount }: OverlayShowOptions) {
    hide();
    const mine = seq;
    void loadDriver().then(({ driver }) => {
      if (mine !== seq || !anchorEl) return;
      const sheet = isSheetViewport();
      const reduced = prefersReducedMotion();
      instance = driver({
        allowClose: false,
        allowKeyboardControl: false,
        overlayClickBehavior: 'none',
        stagePadding: 8,
        stageRadius: 12,
        overlayOpacity: 0.6,
        animate: !reduced,
        smoothScroll: !reduced,
        popoverClass: sheet
          ? 'tour-driver-popover tour-sheet-mode'
          : 'tour-driver-popover',
        onHighlighted: () => {
          instance?.refresh();
          clampPopover();
        },
        onPopoverRender: (popover) => {
          const { wrapper } = popover;
          ['role', 'aria-modal', 'aria-labelledby', 'aria-describedby'].forEach(
            (attr) => wrapper.removeAttribute(attr),
          );
          [
            popover.title,
            popover.description,
            popover.footer,
            popover.closeButton,
            popover.progress,
          ].forEach((el) => {
            el.hidden = true;
          });
          const host = document.createElement('div');
          host.className = 'tour-host tour-host--driver';
          host.dataset.tourHost = 'driver';
          wrapper.appendChild(host);
          unmount?.();
          const release = mount(host);
          const resize =
            typeof ResizeObserver === 'function'
              ? new ResizeObserver(() => {
                  instance?.refresh();
                  clampPopover();
                })
              : null;
          resize?.observe(host);
          unmount = () => {
            resize?.disconnect();
            release();
            host.remove();
          };
        },
      });
      instance.highlight({
        element: anchorEl,
        popover: { title: ' ', description: ' ', side: 'bottom', align: 'start' },
      });
      room.watch(anchorEl, sheet, () => {
        instance?.refresh();
        clampPopover();
      });
    });
  }

  return {
    show,
    refresh() {
      instance?.refresh();
    },
    hide,
  };
}
