import { getAgentStatus } from '@/actions/tmgr/agentStatus';
import {
	applyAgentStatusEvent,
	trackAgentStatus,
	useTaskAgentStatus,
} from '../useAgentStatus';
import type { AgentSession } from '@/utils/agentStatus';

jest.mock('@/actions/tmgr/agentStatus', () => ({ getAgentStatus: jest.fn() }));

const mocked = getAgentStatus as jest.Mock;

const session = (overrides: Partial<AgentSession> = {}): AgentSession => ({
	session_id: 'sess-0001',
	workspace_id: 5,
	task: { id: 10, key: 'TM-10' },
	state: 'working',
	state_since: null,
	started_at: null,
	updated_at: null,
	ended_at: null,
	agent_profile: 'claude-code',
	host_label: null,
	user: null,
	stale: false,
	...overrides,
});

const makePusher = () => {
	let handlers: any = null;
	return {
		emit: (s: AgentSession) => handlers.onAgentStatusChanged(s),
		subscribeToWorkspace: jest.fn((_ws: number, h: any) => {
			handlers = h;
			return 'sub-1';
		}),
		unsubscribeHandlerFromWorkspace: jest.fn(),
	};
};

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('trackAgentStatus', () => {
	beforeEach(() => {
		jest.useRealTimers();
		mocked.mockReset();
	});

	it('loads once per workspace, applies events and unsubscribes on the last release', async () => {
		mocked.mockResolvedValue([session()]);
		const pusher = makePusher();
		const status = useTaskAgentStatus(
			() => 5,
			() => 10,
		);
		const releaseA = trackAgentStatus(5, pusher);
		const releaseB = trackAgentStatus(5, pusher);
		await flush();
		expect(mocked).toHaveBeenCalledTimes(1);
		expect(pusher.subscribeToWorkspace).toHaveBeenCalledTimes(1);
		expect(status.value?.state).toBe('working');

		pusher.emit(session({ state: 'blocked' }));
		expect(status.value?.state).toBe('blocked');
		pusher.emit(session({ ended_at: '2026-10-13T12:00:00Z' }));
		expect(status.value).toBeNull();

		releaseA();
		expect(pusher.unsubscribeHandlerFromWorkspace).not.toHaveBeenCalled();
		releaseB();
		expect(pusher.unsubscribeHandlerFromWorkspace).toHaveBeenCalledWith(5, 'sub-1');
	});

	it('shows nothing when the workspace answers 403', async () => {
		mocked.mockRejectedValue({ response: { status: 403 } });
		const pusher = makePusher();
		const status = useTaskAgentStatus(
			() => 7,
			() => 10,
		);
		const release = trackAgentStatus(7, pusher);
		await flush();
		applyAgentStatusEvent(7, session({ workspace_id: 7 }));
		expect(status.value).toBeNull();
		release();
	});

	it('does not track local workspaces', () => {
		const pusher = makePusher();
		trackAgentStatus(-3, pusher)();
		expect(pusher.subscribeToWorkspace).not.toHaveBeenCalled();
		expect(mocked).not.toHaveBeenCalled();
	});
});
