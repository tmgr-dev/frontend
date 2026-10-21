jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { patch: jest.fn() },
}));
jest.mock('@/store', () => ({
	__esModule: true,
	default: { state: {}, getters: {}, commit: jest.fn() },
}));
import axios from '@/plugins/axios';
import { rescheduleRoutineInstance } from '../daily-tasks';

beforeEach(() => {
	jest.clearAllMocks();
	(axios.patch as jest.Mock).mockResolvedValue({ data: { data: {} } });
});

test('the legacy body is unchanged without an origin date', async () => {
	await rescheduleRoutineInstance(5, 'virtual', '2026-10-13 09:00:00');
	expect(axios.patch).toHaveBeenCalledWith(
		'daily-routines/tasks/5/instances/virtual',
		{ scheduled_for: '2026-10-13 09:00:00' },
	);
});

test('the origin date is sent as from_date', async () => {
	await rescheduleRoutineInstance(5, 77, '2026-10-14 09:00:00', '2026-10-13');
	expect(axios.patch).toHaveBeenCalledWith(
		'daily-routines/tasks/5/instances/77',
		{ scheduled_for: '2026-10-14 09:00:00', from_date: '2026-10-13' },
	);
});
