import { getAgentStatus } from '@/actions/tmgr/agentStatus';
import {
	groupByTask,
	replaceSessions,
	upsertSession,
	type AgentSession,
	type TaskAgentStatus,
} from '@/utils/agentStatus';
import { computed, reactive } from 'vue';

export interface AgentStatusPusher {
	subscribeToWorkspace: (
		workspaceId: number,
		handlers: { onAgentStatusChanged?: (session: AgentSession) => void },
	) => string;
	unsubscribeHandlerFromWorkspace: (
		workspaceId: number,
		subscriptionId: string,
	) => void;
}

interface Tracker {
	refs: number;
	timer: ReturnType<typeof setInterval> | null;
	subscriptionId: string | null;
	onVisible: (() => void) | null;
	request: number;
}

const REFRESH_MS = 60_000;

const sessionsByWorkspace = reactive<Record<number, AgentSession[]>>({});
const disabledWorkspaces = reactive<Record<number, boolean>>({});
const trackers = new Map<number, Tracker>();

const isTabVisible = (): boolean =>
	typeof document === 'undefined' || document.visibilityState !== 'hidden';

const stopTimer = (tracker: Tracker) => {
	if (tracker.timer) clearInterval(tracker.timer);
	tracker.timer = null;
};

const load = async (workspaceId: number, tracker: Tracker) => {
	const current = ++tracker.request;
	try {
		const snapshot = await getAgentStatus(workspaceId);
		if (current !== tracker.request || disabledWorkspaces[workspaceId]) return;
		sessionsByWorkspace[workspaceId] = replaceSessions(snapshot);
	} catch (error: any) {
		if (error?.response?.status === 403) {
			disabledWorkspaces[workspaceId] = true;
			sessionsByWorkspace[workspaceId] = [];
			stopTimer(tracker);
		}
	}
};

export const applyAgentStatusEvent = (
	workspaceId: number,
	session: AgentSession,
) => {
	if (!session?.session_id) return;
	sessionsByWorkspace[workspaceId] = upsertSession(
		sessionsByWorkspace[workspaceId] ?? [],
		session,
	);
};

export const trackAgentStatus = (
	workspaceId: number,
	pusher: AgentStatusPusher,
): (() => void) => {
	if (!(workspaceId > 0)) return () => {};
	let tracker = trackers.get(workspaceId);
	if (!tracker) {
		tracker = {
			refs: 0,
			timer: null,
			subscriptionId: null,
			onVisible: null,
			request: 0,
		};
		trackers.set(workspaceId, tracker);
		const created = tracker;
		disabledWorkspaces[workspaceId] = false;
		created.subscriptionId = pusher.subscribeToWorkspace(workspaceId, {
			onAgentStatusChanged: (session) =>
				applyAgentStatusEvent(workspaceId, session),
		});
		created.timer = setInterval(() => {
			if (isTabVisible()) load(workspaceId, created);
		}, REFRESH_MS);
		if (typeof document !== 'undefined') {
			created.onVisible = () => {
				if (isTabVisible() && !disabledWorkspaces[workspaceId]) {
					load(workspaceId, created);
				}
			};
			document.addEventListener('visibilitychange', created.onVisible);
		}
		load(workspaceId, created);
	}
	tracker.refs += 1;
	const active = tracker;
	let released = false;
	return () => {
		if (released) return;
		released = true;
		active.refs -= 1;
		if (active.refs > 0) return;
		stopTimer(active);
		if (active.onVisible) {
			document.removeEventListener('visibilitychange', active.onVisible);
		}
		if (active.subscriptionId) {
			pusher.unsubscribeHandlerFromWorkspace(
				workspaceId,
				active.subscriptionId,
			);
		}
		trackers.delete(workspaceId);
		delete sessionsByWorkspace[workspaceId];
	};
};

export const useTaskAgentStatus = (
	workspaceId: () => number | null | undefined,
	taskId: () => number | null | undefined,
) =>
	computed<TaskAgentStatus | null>(() => {
		const ws = workspaceId();
		const id = taskId();
		if (!ws || id == null || disabledWorkspaces[ws]) return null;
		return groupByTask(sessionsByWorkspace[ws] ?? [])[id] ?? null;
	});
