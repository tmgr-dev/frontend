import type { PopoverExtras } from '@/popover/types';
import type { RunnerSession } from '@/runner/api';
import { isDesktopApp, supportsToastWindow } from '@/utils/desktop';
import { isDndActive } from '@/utils/dnd';

export const LIMIT_THRESHOLD = 95;

export interface ToastPayload {
	id: string;
	kind: 'decision' | 'agent' | 'idle' | 'limit';
	title: string;
	body: string;
	meta?: string;
	actions: { id: string; label: string; primary?: boolean }[];
	ttlMs?: number | null;
	data?: Record<string, unknown>;
}

const WINDOW_LABELS: Record<string, string> = {
	fiveHour: '5h',
	week: 'Weekly',
};

const durationText = (seconds: number): string => {
	const minutes = Math.max(0, Math.floor(seconds / 60));
	const hours = Math.floor(minutes / 60);
	const rest = String(minutes % 60).padStart(2, '0');
	return hours > 0 ? `${hours}h ${rest}m` : `${minutes % 60}m`;
};

export const resetText = (
	resetsAt: number | null,
	nowSeconds: number,
): string => {
	if (resetsAt === null) return 'resets soon';
	const minutes = Math.max(1, Math.ceil((resetsAt - nowSeconds) / 60));
	return minutes >= 120
		? `resets in ${Math.round(minutes / 60)} h`
		: `resets in ${minutes} min`;
};

export const limitKey = (window: string, resetsAt: number | null): string =>
	`${window}:${resetsAt ?? 'none'}`;

export const limitToasts = (
	usage: PopoverExtras['usage'] | undefined,
	seen: ReadonlySet<string>,
	nowSeconds: number,
): { key: string; toast: ToastPayload }[] => {
	const toasts: { key: string; toast: ToastPayload }[] = [];
	const claude = usage?.providers?.find((p) => p.tool === 'claude');
	for (const window of ['fiveHour', 'week'] as const) {
		const bar = claude?.[window];
		if (!bar || bar.pct < LIMIT_THRESHOLD) continue;
		const key = limitKey(window, bar.resetsAt);
		if (seen.has(key)) continue;
		toasts.push({
			key,
			toast: {
				id: `limit:${key}`,
				kind: 'limit',
				title: `Claude limit ${Math.round(bar.pct)}%`,
				body: `${WINDOW_LABELS[window]} window ${resetText(bar.resetsAt, nowSeconds)}.`,
				actions: [{ id: 'ok', label: 'OK', primary: true }],
			},
		});
	}
	return toasts;
};

export const agentFinishedToasts = (
	previous: Map<string, string>,
	sessions: RunnerSession[],
	nowMs: number,
): { toasts: ToastPayload[]; next: Map<string, string> } => {
	const next = new Map<string, string>();
	const toasts: ToastPayload[] = [];
	for (const session of sessions) {
		next.set(session.id, session.state.kind);
		const before = previous.get(session.id);
		const finished =
			before !== undefined &&
			before !== 'exited' &&
			session.state.kind === 'exited' &&
			session.state.detail === 'done';
		if (!finished || !session.task) continue;
		toasts.push({
			id: `agent:${session.id}`,
			kind: 'agent',
			title: 'Agent finished',
			body: `${session.name} finished its run.`,
			meta: durationText((nowMs - session.createdMs) / 1000),
			actions: [{ id: 'open-task', label: 'Open task', primary: true }],
			data: {
				workspaceId: session.task.workspaceId,
				taskId: session.task.taskId,
			},
		});
	}
	return { toasts, next };
};

export const pushToast = async (toast: ToastPayload): Promise<boolean> => {
	if (!isDesktopApp() || !supportsToastWindow() || isDndActive()) return false;
	try {
		const { invoke } = await import('@tauri-apps/api/core');
		await invoke('toast_push', { toast });
		return true;
	} catch (error) {
		console.error('toast_push failed', error);
		return false;
	}
};
