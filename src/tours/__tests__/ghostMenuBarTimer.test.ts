import { GHOST_HEADER } from '@/local/ghostHeader';
import { installDesktopTriggers } from '../desktopTriggers';
import { createTourBus, installTourInterceptor } from '../events';

const makeAxios = () => {
	let handler: (r: any) => any = (r) => r;
	return {
		axios: {
			interceptors: {
				response: {
					use: (ok: (r: any) => any) => {
						handler = ok;
						return 1;
					},
					eject: () => {},
				},
			},
		} as any,
		run: (r: any) => handler(r),
	};
};

const setup = () => {
	const bus = createTourBus();
	const { axios, run } = makeAxios();
	installTourInterceptor(axios, () => undefined, bus);
	const offer = jest.fn().mockResolvedValue(true);
	installDesktopTriggers(
		bus,
		{ offer },
		{ isDesktop: () => true, windowLabel: () => 'main' },
	);
	const startTimer = (headers?: Record<string, string>) =>
		run({
			status: 200,
			config: { method: 'post', url: 'tasks/7/countdown', headers },
			data: { data: { id: 7 } },
		});
	return { offer, startTimer };
};

describe('the menu bar tour and the demo timer', () => {
	it('is not offered when the ghost starts its timer', () => {
		const { offer, startTimer } = setup();
		startTimer({ [GHOST_HEADER]: '1' });
		expect(offer).not.toHaveBeenCalled();
	});

	it('is offered when the user starts a timer', () => {
		const { offer, startTimer } = setup();
		startTimer();
		expect(offer).toHaveBeenCalledWith('desktop.menu-bar-timer', 'event', {
			retryMs: 60_000,
		});
	});
});
