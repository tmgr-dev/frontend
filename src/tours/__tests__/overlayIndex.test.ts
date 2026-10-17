/** @jest-environment jsdom */
const driver = { show: jest.fn(), refresh: jest.fn(), hide: jest.fn() };
const passive = { show: jest.fn(), refresh: jest.fn(), hide: jest.fn() };

jest.mock('../overlay/driverAdapter', () => ({
	createDriverAdapter: () => driver,
}));
jest.mock('../overlay/passiveAdapter', () => ({
	createPassiveAdapter: () => passive,
}));

import { createOverlayAdapter, registerOverlay } from '../overlay';

const mount = () => () => {};
const anchor = document.createElement('button');

beforeEach(() => {
	[driver, passive].forEach((a) =>
		Object.values(a).forEach((f) => f.mockClear()),
	);
});

describe('createOverlayAdapter', () => {
	it('uses driver for modal steps with an anchor', () => {
		const a = createOverlayAdapter();
		a.show({ anchorEl: anchor, mode: 'modal', mount });
		expect(driver.show).toHaveBeenCalledTimes(1);
		expect(passive.show).not.toHaveBeenCalled();
	});

	it('uses the passive adapter for wait steps and centered mode', () => {
		const a = createOverlayAdapter();
		a.show({ anchorEl: anchor, mode: 'passive', mount });
		a.show({ anchorEl: null, mode: 'centered', mount });
		a.show({ anchorEl: null, mode: 'modal', mount });
		expect(passive.show).toHaveBeenCalledTimes(3);
		expect(driver.show).not.toHaveBeenCalled();
	});

	it('hides the previous adapter when switching and delegates refresh/hide', () => {
		const a = createOverlayAdapter();
		a.show({ anchorEl: anchor, mode: 'modal', mount });
		a.show({ anchorEl: anchor, mode: 'passive', mount });
		expect(driver.hide).toHaveBeenCalledTimes(1);
		a.refresh();
		expect(passive.refresh).toHaveBeenCalled();
		a.hide();
		expect(passive.hide).toHaveBeenCalled();
	});

	it('registerOverlay hands the adapter to the setter', () => {
		const set = jest.fn();
		const adapter = registerOverlay(set);
		expect(set).toHaveBeenCalledWith(adapter);
	});
});
