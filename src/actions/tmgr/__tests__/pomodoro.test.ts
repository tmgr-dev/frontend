jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn(), post: jest.fn(), put: jest.fn() },
}));
jest.mock('@/store', () => ({ __esModule: true, default: { state: {} } }));
jest.mock('@/actions/tmgr/user', () => ({ patchUserSettings: jest.fn() }));

import $axios from '@/plugins/axios';
import {
	distractPomodoro,
	enablePomodoro,
	getPomodoroState,
	updatePomodoroState,
} from '../pomodoro';

const http = $axios as unknown as Record<'get' | 'post' | 'put', jest.Mock>;
const state = { id: 1, task_id: 7, phase: 'focus', remaining_ms: 1000 };
const reply = (data: unknown) => ({ data: { data } });
let taskId = 100;

describe('pomodoro state normalization', () => {
	it.each([[[]], [null], [undefined], ['x'], [5]])(
		'GET answering %p reads as not enabled',
		async (data) => {
			http.get.mockResolvedValueOnce(reply(data));
			await expect(getPomodoroState(taskId++)).resolves.toBeNull();
		},
	);

	it('GET passes a state object through', async () => {
		http.get.mockResolvedValueOnce(reply(state));
		await expect(getPomodoroState(taskId++)).resolves.toEqual(state);
	});

	it('mutations answering [] read as null', async () => {
		http.post.mockResolvedValue(reply([]));
		http.put.mockResolvedValue(reply([]));
		await expect(enablePomodoro(1)).resolves.toBeNull();
		await expect(updatePomodoroState(1, { running: true })).resolves.toBeNull();
		await expect(distractPomodoro(1)).resolves.toBeNull();
		http.post.mockResolvedValue(reply(state));
		await expect(enablePomodoro(1)).resolves.toEqual(state);
	});
});
