import { installDesktopTriggers } from '../desktopTriggers';
import { createTourBus } from '../events';

const setup = (isDesktop: boolean, label: string | null) => {
	const bus = createTourBus();
	const offer = jest.fn().mockResolvedValue(true);
	const off = installDesktopTriggers(
		bus,
		{ offer },
		{ isDesktop: () => isDesktop, windowLabel: () => label },
	);
	return { bus, offer, off };
};

describe('desktop triggers', () => {
	it('offers the menu bar tour when a timer starts in the main window', () => {
		const { bus, offer } = setup(true, 'main');
		bus.emit('timer.started', { taskId: 1 });
		expect(offer).toHaveBeenCalledWith('desktop.menu-bar-timer', 'event', {
			retryMs: 60_000,
		});
	});

	it('does nothing in other windows or on the web', () => {
		for (const [desktop, label] of [
			[true, 'quick-add'],
			[true, null],
			[false, 'main'],
		] as const) {
			const { bus, offer } = setup(desktop, label);
			bus.emit('timer.started', {});
			expect(offer).not.toHaveBeenCalled();
		}
	});

	it('stops listening after cleanup', () => {
		const { bus, offer, off } = setup(true, 'main');
		off();
		bus.emit('timer.started', {});
		expect(offer).not.toHaveBeenCalled();
	});
});
