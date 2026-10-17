import type { OverlayAdapter, OverlayShowOptions } from './OverlayAdapter';
import { createDriverAdapter } from './driverAdapter';
import { createPassiveAdapter } from './passiveAdapter';

export type { OverlayAdapter, OverlayShowOptions } from './OverlayAdapter';

export function createOverlayAdapter(): OverlayAdapter {
	const driver = createDriverAdapter();
	const passive = createPassiveAdapter();
	let current: OverlayAdapter | null = null;

	return {
		show(options: OverlayShowOptions) {
			const next =
				options.mode === 'modal' && options.anchorEl ? driver : passive;
			if (current && current !== next) current.hide();
			current = next;
			next.show(options);
		},
		refresh() {
			current?.refresh();
		},
		hide() {
			current?.hide();
			current = null;
		},
	};
}

export function registerOverlay(
	setOverlayAdapter: (adapter: OverlayAdapter) => void,
): OverlayAdapter {
	const adapter = createOverlayAdapter();
	setOverlayAdapter(adapter);
	return adapter;
}
