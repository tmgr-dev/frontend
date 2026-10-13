const invoke = jest.fn();
class FakeChannel {
	onmessage: ((m: unknown) => void) | undefined;
}
jest.mock('@tauri-apps/api/core', () => ({
	invoke: (...args: unknown[]) => invoke(...args),
	Channel: FakeChannel,
}));

import * as api from '../api';

beforeEach(() => {
	invoke.mockReset();
	invoke.mockResolvedValue(null);
});

describe('runner api wrappers', () => {
	it.each([
		['runnerEnabled', [], 'runner_enabled', undefined],
		['runnerSetEnabled', [true], 'runner_set_enabled', { enabled: true }],
		['runnerStatus', [], 'runner_status', undefined],
		['runnerProfiles', [], 'runner_profiles', undefined],
		['runnerSessions', [], 'runner_sessions', undefined],
		['runnerHalt', ['s_a'], 'runner_halt', { session: 's_a' }],
		['runnerOpenTerminal', ['s_a'], 'runner_open_terminal', { session: 's_a' }],
		['runnerUnwatch', [3], 'runner_unwatch', { id: 3 }],
		['runnerTermAck', [4], 'runner_term_ack', { seq: 4 }],
		['runnerTermInput', ['x'], 'runner_term_input', { data: 'x' }],
		['runnerTermResize', [80, 24], 'runner_term_resize', { cols: 80, rows: 24 }],
		['runnerTermScroll', [5], 'runner_term_scroll', { offset: 5 }],
		['runnerTermDetach', [], 'runner_term_detach', undefined],
		['runnerTermOpenLink', ['https://a.b'], 'runner_term_open_link', { url: 'https://a.b' }],
	])('%s -> %s', async (fn, args, command, payload) => {
		await (api as any)[fn](...args);
		expect(invoke).toHaveBeenCalledWith(
			...(payload === undefined ? [command] : [command, payload]),
		);
	});

	it('runnerStartRun sends the contract fields', async () => {
		const args = { workspaceId: -1000, taskId: 7, taskTitle: 'T', profile: 'codex' };
		await api.runnerStartRun(args);
		expect(invoke).toHaveBeenCalledWith('runner_start_run', args);
	});

	it('watch and attach pass a channel wired to the handler', async () => {
		const handler = jest.fn();
		await api.runnerWatch(handler);
		const [command, payload] = invoke.mock.calls[0];
		expect(command).toBe('runner_watch');
		payload.onEvent.onmessage({ type: 'down', message: 'x' });
		expect(handler).toHaveBeenCalledWith({ type: 'down', message: 'x' });

		invoke.mockClear();
		await api.runnerTermAttach(handler, 100, 30);
		const [cmd, args] = invoke.mock.calls[0];
		expect(cmd).toBe('runner_term_attach');
		expect(args).toMatchObject({ cols: 100, rows: 30 });
		expect(args.onFrame).toBeInstanceOf(FakeChannel);
	});
});
