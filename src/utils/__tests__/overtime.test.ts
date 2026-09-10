import {
	pageOvertimeSeconds,
	taskEstimateSeconds,
	taskOvertimeSeconds,
	totalOvertimeSeconds,
} from '../overtime';

describe('overtime', () => {
	const over = { common_time: 7200, approximately_time: 3600 };
	const under = { common_time: 1800, approximately_time: 3600 };
	const fromSetting = { common_time: 5400, settings: [{ key: 'approximately_time', value: '3600' }] };
	const noEstimate = { common_time: 9999 };

	it('takes the estimate from the task, then from the category setting', () => {
		expect(taskEstimateSeconds(over)).toBe(3600);
		expect(taskEstimateSeconds(fromSetting)).toBe(3600);
		expect(taskEstimateSeconds({ settings: [{ key: 'approximately_time', pivot: { value: 600 } }] })).toBe(600);
		expect(taskEstimateSeconds(noEstimate)).toBe(0);
		expect(taskEstimateSeconds({ approximately_time: 'abc' })).toBe(0);
	});

	it('counts only time beyond a positive estimate', () => {
		expect(taskOvertimeSeconds(over)).toBe(3600);
		expect(taskOvertimeSeconds(under)).toBe(0);
		expect(taskOvertimeSeconds(fromSetting)).toBe(1800);
		expect(taskOvertimeSeconds(noEstimate)).toBe(0);
		expect(pageOvertimeSeconds([over, under, fromSetting, noEstimate])).toBe(5400);
	});

	it('prefers the server total over the page sum', () => {
		expect(totalOvertimeSeconds({ total_overtime_seconds: '108000' }, [over])).toBe(108000);
		expect(totalOvertimeSeconds({ total_overtime_seconds: 0 }, [over])).toBe(0);
	});

	it('falls back to the page sum when the server total is missing', () => {
		expect(totalOvertimeSeconds(undefined, [over, fromSetting])).toBe(5400);
		expect(totalOvertimeSeconds({}, [over])).toBe(3600);
		expect(totalOvertimeSeconds({ total_overtime_seconds: '' }, [over])).toBe(3600);
	});
});
