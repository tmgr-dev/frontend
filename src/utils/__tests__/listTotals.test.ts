import {
	formatTaskCount,
	listTotalSeconds,
	sumTaskSeconds,
} from '@/utils/listTotals';

describe('formatTaskCount', () => {
	it('uses the singular only for one task', () => {
		expect(formatTaskCount(0)).toBe('0 tasks');
		expect(formatTaskCount(1)).toBe('1 task');
		expect(formatTaskCount(2)).toBe('2 tasks');
		expect(formatTaskCount(98)).toBe('98 tasks');
	});

	it('shows 0 tasks for a missing or broken count', () => {
		expect(formatTaskCount(undefined)).toBe('0 tasks');
		expect(formatTaskCount(null)).toBe('0 tasks');
		expect(formatTaskCount(NaN)).toBe('0 tasks');
	});
});

describe('sumTaskSeconds', () => {
	it('sums tracked time and treats missing time as zero', () => {
		expect(
			sumTaskSeconds([
				{ common_time: 3600 },
				{ common_time: null },
				{},
				{ common_time: 1800 },
			]),
		).toBe(5400);
		expect(sumTaskSeconds([])).toBe(0);
	});
});

describe('listTotalSeconds', () => {
	it('uses the page sum when the server sends no total', () => {
		expect(listTotalSeconds(undefined, 5400, 0)).toBe(5400);
	});

	it('uses the server total, which arrives as a string, when nothing changed', () => {
		expect(listTotalSeconds('36000', 5400, 5400)).toBe(36000);
	});

	it('moves the server total by what changed on the page since the load', () => {
		// a timer stop added 10 minutes to a visible task
		expect(listTotalSeconds('36000', 6000, 5400)).toBe(36600);
		// a task with 1 hour left the page
		expect(listTotalSeconds('36000', 1800, 5400)).toBe(32400);
	});

	it('never goes below zero', () => {
		expect(listTotalSeconds('600', 0, 5400)).toBe(0);
	});
});
