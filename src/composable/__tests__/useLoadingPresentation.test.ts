import { effectScope, ref } from 'vue';
import { useLoadingPresentation } from '../useLoadingPresentation';

describe('loading presentation', () => {
	beforeEach(() => jest.useFakeTimers());
	afterEach(() => jest.useRealTimers());
	function setup() {
		const pending = ref(true),
			loaded = ref(false),
			error = ref<unknown>(null);
		const scope = effectScope();
		const state = scope.run(() =>
			useLoadingPresentation({ pending, loaded, error }),
		)!;
		return { pending, loaded, error, scope, ...state };
	}
	it('reserves the initial shape immediately but never delays ready content', () => {
		const s = setup();
		expect(s.showSkeleton.value).toBe(true);
		expect(s.showIndicator.value).toBe(false);
		s.loaded.value = true;
		s.pending.value = false;
		jest.advanceTimersByTime(200);
		expect(s.showSkeleton.value).toBe(false);
		expect(s.showIndicator.value).toBe(false);
		s.scope.stop();
	});
	it('keeps successful empty content during refresh and delays the indicator', () => {
		const s = setup();
		s.loaded.value = true;
		expect(s.showSkeleton.value).toBe(false);
		jest.advanceTimersByTime(119);
		expect(s.showIndicator.value).toBe(false);
		jest.advanceTimersByTime(1);
		expect(s.showIndicator.value).toBe(true);
		s.pending.value = false;
		expect(s.showIndicator.value).toBe(false);
		s.scope.stop();
	});
	it('a new operation cannot inherit an old visual timer', () => {
		const s = setup();
		jest.advanceTimersByTime(100);
		s.pending.value = false;
		s.pending.value = true;
		jest.advanceTimersByTime(20);
		expect(s.showIndicator.value).toBe(false);
		jest.advanceTimersByTime(100);
		expect(s.showIndicator.value).toBe(true);
		s.scope.stop();
	});
	it('ends skeleton on error and clears timers on disposal', () => {
		const s = setup();
		s.error.value = 'Offline';
		expect(s.showSkeleton.value).toBe(false);
		s.scope.stop();
		expect(jest.getTimerCount()).toBe(0);
	});
});
