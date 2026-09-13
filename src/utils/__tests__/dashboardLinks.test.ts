import { dashboardStatisticTarget, taskTarget } from '../dashboardLinks';

describe('dashboardStatisticTarget', () => {
	it('sends time tracking to the stats page', () => {
		expect(dashboardStatisticTarget({ view: 'time_tracking' })).toEqual({ path: '/stats' });
	});

	it('uses the route daily routines actually live at', () => {
		expect(dashboardStatisticTarget({ view: 'daily_routine' })).toEqual({ path: '/daily-routines' });
	});

	it('sends a status filter to the task list, not to a route that does not exist', () => {
		expect(dashboardStatisticTarget({ status: 'active', period: 'week' }))
			.toEqual({ path: '/list', query: { status: 'active', period: 'week' } });
	});

	it('drops an all-status filter instead of passing it along', () => {
		expect(dashboardStatisticTarget({ status: 'all' })).toEqual({ path: '/list', query: {} });
	});

	it('stays put for a view that has no page yet rather than landing on a bogus one', () => {
		expect(dashboardStatisticTarget({ view: 'team' })).toBeNull();
	});
});

describe('taskTarget', () => {
	it('opens a task in the modal the rest of the app uses', () => {
		expect(taskTarget(42)).toEqual({ taskId: 42 });
	});

	it('says nothing to open when there is no task', () => {
		expect(taskTarget(null)).toBeNull();
		expect(taskTarget(undefined)).toBeNull();
		expect(taskTarget(0)).toBeNull();
	});
});
