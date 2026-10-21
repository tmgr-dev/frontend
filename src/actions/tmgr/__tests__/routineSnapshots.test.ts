jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: {
		post: jest.fn(),
		put: jest.fn(),
		patch: jest.fn(),
		delete: jest.fn(),
	},
}));
import axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';
import {
	archiveDailyTask,
	completeDailyTask,
	completeDailyTaskInstance,
	completeRoutineOn,
	convertRoutineToTask,
	createDailyTask,
	deleteDailyTask,
	deleteDailyTaskInstance,
	importRoutinesIcs,
	quickCreateRoutine,
	rescheduleRoutineInstance,
	updateDailyTask,
} from '../daily-tasks';

const key = requestCache.generateKey('routines-snapshot', {
	workspace: 1,
	from: '2026-10-21',
	to: '2026-10-21',
});
const other = 'dashboard-snapshot-statistics';

beforeEach(() => {
	requestCache.clear();
	requestCache.set(key, { entries: [] });
	requestCache.set(other, { total: 1 });
	jest.clearAllMocks();
	for (const method of ['post', 'put', 'patch', 'delete'] as const)
		(axios[method] as jest.Mock).mockResolvedValue({ data: { data: {} } });
});

test.each([
	['createDailyTask', () => createDailyTask({ title: 'Walk' } as any)],
	['updateDailyTask', () => updateDailyTask(5, { title: 'Walk' } as any)],
	['completeDailyTask', () => completeDailyTask(5)],
	['archiveDailyTask', () => archiveDailyTask(5)],
	['deleteDailyTask', () => deleteDailyTask(5)],
	['deleteDailyTaskInstance', () => deleteDailyTaskInstance(5, 6)],
	['completeDailyTaskInstance', () => completeDailyTaskInstance(5, 6)],
	['completeRoutineOn', () => completeRoutineOn(5, '2026-10-21')],
	[
		'rescheduleRoutineInstance',
		() => rescheduleRoutineInstance(5, 'virtual', '2026-10-22 09:00:00'),
	],
	['quickCreateRoutine', () => quickCreateRoutine({ title: 'Walk' })],
	[
		'importRoutinesIcs',
		() => importRoutinesIcs(new Blob(['BEGIN:VCALENDAR']) as File),
	],
	['convertRoutineToTask', () => convertRoutineToTask(5, { workspace_id: 1 })],
])('%s drops routine snapshots only', async (_name, write) => {
	await write();
	expect(requestCache.get(key)).toBeNull();
	expect(requestCache.get(other)).toEqual({ total: 1 });
});

test('a failed write keeps the snapshot', async () => {
	(axios.delete as jest.Mock).mockRejectedValue(new Error('nope'));
	jest.spyOn(console, 'error').mockImplementation(() => {});
	await expect(deleteDailyTask(5)).rejects.toThrow('nope');
	expect(requestCache.get(key)).toEqual({ entries: [] });
});
