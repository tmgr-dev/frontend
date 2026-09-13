import { applyTimerState, timerStateDiffers } from '../timerSync';

describe('timerStateDiffers', () => {
	it('sees a timer that was started in another tab', () => {
		expect(timerStateDiffers({ start_time: null, common_time: 120 }, { start_time: 1757700000, common_time: 120 })).toBe(true);
	});

	it('sees a timer that was stopped in another tab', () => {
		expect(timerStateDiffers({ start_time: 1757700000, common_time: 120 }, { start_time: null, common_time: 300 })).toBe(true);
	});

	it('stays quiet when the timer did not move', () => {
		expect(timerStateDiffers({ start_time: 1757700000, common_time: 120 }, { start_time: 1757700000, common_time: 120 })).toBe(false);
	});

	it('treats a missing value and a null the same way', () => {
		expect(timerStateDiffers({ common_time: 0 }, { start_time: null, common_time: 0 })).toBe(false);
	});

	it('compares numbers a string would hide', () => {
		expect(timerStateDiffers({ start_time: 1757700000 }, { start_time: '1757700000' as unknown as number })).toBe(false);
	});
});

describe('applyTimerState', () => {
	it('copies the running timer onto the form', () => {
		const form = { id: 5, start_time: null, common_time: 120 };

		expect(applyTimerState(form, { id: 5, start_time: 1757700000, common_time: 120 })).toBe(true);
		expect(form.start_time).toBe(1757700000);
	});

	it('clears the timer when the other tab stopped it, keeping the tracked total', () => {
		const form = { id: 5, start_time: 1757700000, common_time: 120 };

		expect(applyTimerState(form, { id: 5, start_time: null, common_time: 300 })).toBe(true);
		expect(form.start_time).toBeNull();
		expect(form.common_time).toBe(300);
	});

	it('ignores an event about another task', () => {
		const form = { id: 5, start_time: null, common_time: 120 };

		expect(applyTimerState(form, { id: 6, start_time: 1757700000, common_time: 0 })).toBe(false);
		expect(form.start_time).toBeNull();
	});

	it('ignores an event with no task', () => {
		const form = { id: 5, start_time: null, common_time: 120 };

		expect(applyTimerState(form, null)).toBe(false);
		expect(form.start_time).toBeNull();
	});

	it('leaves the form alone when nothing changed', () => {
		const form = { id: 5, start_time: 1757700000, common_time: 120 };

		expect(applyTimerState(form, { id: 5, start_time: 1757700000, common_time: 120 })).toBe(false);
	});
});
