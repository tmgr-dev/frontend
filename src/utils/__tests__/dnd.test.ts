import { dndState, isDndActive, setDnd } from '../dnd';

describe('dnd', () => {
	let errorSpy: jest.SpyInstance;
	beforeEach(() => {
		errorSpy = jest.spyOn(console, 'error').mockImplementation(() => undefined);
	});
	afterEach(() => {
		dndState.option = 'off';
		dndState.until = null;
		errorSpy.mockRestore();
	});

	it('is inactive when off', () => {
		expect(isDndActive()).toBe(false);
	});

	it('is active for the chosen duration, then expires', () => {
		const now = Date.now();
		setDnd('1h');
		expect(dndState.option).toBe('1h');
		expect(isDndActive(now)).toBe(true);
		expect(isDndActive(now + 61 * 60_000)).toBe(false);
	});

	it('for 3 hours', () => {
		const before = Date.now();
		setDnd('3h');
		expect(dndState.until).not.toBeNull();
		expect(dndState.until! - before).toBeGreaterThan(2 * 60 * 60_000);
		expect(dndState.until! - before).toBeLessThanOrEqual(3 * 60 * 60_000 + 1000);
	});

	it('until tomorrow means 9am the next day', () => {
		setDnd('tomorrow');
		const until = new Date(dndState.until!);
		expect(until.getHours()).toBe(9);
		expect(until.getMinutes()).toBe(0);
		expect(until.getTime()).toBeGreaterThan(Date.now());
	});

	it('turning it off clears the until time', () => {
		setDnd('1h');
		setDnd('off');
		expect(dndState.until).toBeNull();
		expect(isDndActive()).toBe(false);
	});
});
