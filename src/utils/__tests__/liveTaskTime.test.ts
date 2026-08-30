import { liveTaskTime } from '@/utils/liveTaskTime';

describe('liveTaskTime', () => {
	it('returns tracked time as is for an idle task', () => {
		expect(liveTaskTime({ common_time: 600, start_time: 0 }, 1_000_000)).toBe(600);
		expect(liveTaskTime({ common_time: 600, start_time: null }, 1_000_000)).toBe(600);
	});

	it('adds the running slice for a task whose timer is on', () => {
		expect(liveTaskTime({ common_time: 600, start_time: 999_940 }, 1_000_000)).toBe(660);
	});

	it('treats missing tracked time as zero', () => {
		expect(liveTaskTime({ common_time: null, start_time: 999_990 }, 1_000_000)).toBe(10);
		expect(liveTaskTime({}, 1_000_000)).toBe(0);
	});

	it('never goes backwards on clock skew', () => {
		expect(liveTaskTime({ common_time: 600, start_time: 1_000_500 }, 1_000_000)).toBe(600);
	});
});
