import { isSheetViewport, prefersReducedMotion } from '../a11y';
import type { OverlayAdapter, OverlayShowOptions } from './OverlayAdapter';
import { createFrameClamp } from './clamp';
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

	let currentAnchor: Element | null = null;
	const frameClamp = createFrameClamp(
		() =>
			document.querySelector<HTMLElement>(
				'.driver-popover.tour-driver-popover',
			),
		() => currentAnchor,
	);

	function hide() {
		seq += 1;
		frameClamp.stop();
		currentAnchor = null;
		room.release();
		unmount?.();
		unmount = null;
		instance?.destroy();
		instance = null;
	}

	function show({ anchorEl, mount }: OverlayShowOptions) {
		hide();
		const mine = seq;
		void loadDriver().then(({ driver }) => {
			if (mine !== seq || !anchorEl) return;
			currentAnchor = anchorEl;
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
					frameClamp.schedule();
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
									frameClamp.schedule();
									if (isSheetViewport())
										room.revealAvoided(wrapper, currentAnchor);
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
				popover: {
					title: ' ',
					description: ' ',
					side: 'bottom',
					align: 'start',
				},
			});
			frameClamp.start();
			room.watch(
				anchorEl,
				sheet,
				() => {
					instance?.refresh();
					frameClamp.schedule();
				},
				() =>
					document.querySelector<HTMLElement>(
						'.driver-popover.tour-driver-popover',
					),
			);
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
