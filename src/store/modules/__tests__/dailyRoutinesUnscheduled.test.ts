jest.mock('@/actions/tmgr/daily-tasks', () => ({
	completeRoutineOn: jest.fn(),
	updateDailyTask: jest.fn(),
	rescheduleRoutineInstance: jest.fn(),
}));
import {
	completeRoutineOn,
	updateDailyTask,
} from '@/actions/tmgr/daily-tasks';
import { readFileSync } from 'fs';
import { ModuleKind, transpileModule } from 'typescript';
const source = transpileModule(
	readFileSync(require.resolve('../dailyRoutines.js'), 'utf8'),
	{ compilerOptions: { module: ModuleKind.CommonJS } },
).outputText;
const loaded = { exports: {} as any };
new Function('require', 'module', 'exports', source)(
	require,
	loaded,
	loaded.exports,
);
const routines = loaded.exports.default;
const entry = {
	task_id: 5,
	title: 'Read',
	date: '2026-10-08',
	frequency: 'NONE',
	routine_category: 'none',
};
const ctx = () => ({
	state: { lastRange: {} },
	rootState: { user: { id: 1 } },
	dispatch: jest.fn(),
	commit: jest.fn(),
});

beforeEach(() => jest.clearAllMocks());

test('dropping on the unscheduled zone sends unscheduled without a date', async () => {
	await routines.actions.moveRoutine(ctx(), {
		entry,
		date: '2026-10-08',
		allDay: true,
		unscheduled: true,
	});
	const payload = (updateDailyTask as jest.Mock).mock.calls[0][1];
	expect(payload.unscheduled).toBe(true);
	expect(payload).not.toHaveProperty('scheduled_date');
	expect(payload).not.toHaveProperty('scheduled_time');
});

test('dropping on the all-day row stays dated', async () => {
	await routines.actions.moveRoutine(ctx(), {
		entry,
		date: '2026-10-09',
		allDay: true,
	});
	expect(updateDailyTask).toHaveBeenCalledWith(
		5,
		expect.objectContaining({ scheduled_date: '2026-10-09', scheduled_time: null }),
	);
});

test('toggleComplete patches virtual when the instance is gone', async () => {
	(completeRoutineOn as jest.Mock).mockResolvedValue({
		instance_id: null,
		completed: false,
		status: 'PENDING',
	});
	const c = ctx();
	await routines.actions.toggleComplete(c, entry);
	expect(c.commit).toHaveBeenCalledWith(
		'patchEntry',
		expect.objectContaining({
			patch: expect.objectContaining({ virtual: true, instance_id: null }),
		}),
	);
});

test('toggleComplete keeps virtual false when an instance exists', async () => {
	(completeRoutineOn as jest.Mock).mockResolvedValue({
		instance_id: 9,
		completed: true,
		status: 'COMPLETED',
	});
	const c = ctx();
	await routines.actions.toggleComplete(c, entry);
	expect(c.commit.mock.calls[0][1].patch.virtual).toBe(false);
});
