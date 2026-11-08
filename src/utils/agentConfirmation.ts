import { t } from '@/i18n';
import type {
	AgentMessage,
	PendingAction,
	PendingActionStatus,
} from '@/types/agent';

const STATUSES: PendingActionStatus[] = [
	'pending',
	'executing',
	'executed',
	'failed',
	'cancelled',
	'expired',
	'superseded',
];

const expiredText = (): string => t('agents.helpers.confirmation.expired');

const asRecord = (v: unknown): Record<string, unknown> =>
	v && typeof v === 'object' && !Array.isArray(v)
		? (v as Record<string, unknown>)
		: {};

const normalizeFields = (raw: unknown): { label: string; value: string }[] => {
	if (Array.isArray(raw)) {
		return raw.map((f) => ({
			label: String(asRecord(f).label ?? ''),
			value: String(asRecord(f).value ?? ''),
		}));
	}
	return Object.entries(asRecord(raw)).map(([label, value]) => ({
		label,
		value: String(value ?? ''),
	}));
};

export const normalizePendingAction = (raw: unknown): PendingAction => {
	const r = asRecord(raw);
	const preview = asRecord(r.preview);
	const status = String(r.status ?? 'pending').toLowerCase();
	const result = r.result;

	return {
		id: Number(r.id),
		tool: String(r.tool ?? r.tool_name ?? ''),
		risk: String(r.risk ?? '').toLowerCase() === 'high' ? 'high' : 'low',
		status: STATUSES.includes(status as PendingActionStatus)
			? (status as PendingActionStatus)
			: 'pending',
		preview: {
			title: String(preview.title ?? ''),
			fields: normalizeFields(preview.fields),
			body: typeof preview.body === 'string' ? preview.body : null,
		},
		result:
			result && typeof result === 'object'
				? (result as Record<string, unknown>)
				: null,
		expires_at: String(r.expires_at ?? r.expiresAt ?? ''),
	};
};

export const normalizePendingActions = (raw: unknown): PendingAction[] =>
	Array.isArray(raw) ? raw.map(normalizePendingAction) : [];

export const normalizeAgentMessage = (m: AgentMessage): AgentMessage =>
	Array.isArray(m.pending_actions)
		? { ...m, pending_actions: normalizePendingActions(m.pending_actions) }
		: m;

const expiresAtMs = (iso: string): number => new Date(iso).getTime();

const isPastExpiry = (a: PendingAction, nowMs: number): boolean => {
	const at = expiresAtMs(a.expires_at);
	return !Number.isNaN(at) && at <= nowMs;
};

export const openActionIds = (
	actions: PendingAction[],
	nowMs: number = Date.now(),
): number[] =>
	actions
		.filter((a) => a.status === 'pending' && !isPastExpiry(a, nowMs))
		.map((a) => a.id);

export const hasOpenConfirmation = (
	messages: AgentMessage[],
	nowMs: number = Date.now(),
): boolean =>
	messages.some(
		(m) =>
			m.status === 'awaiting' &&
			openActionIds(m.pending_actions ?? [], nowMs).length > 0,
	);

export const expiryHint = (iso: string, nowMs: number = Date.now()): string => {
	const at = expiresAtMs(iso);
	if (Number.isNaN(at)) return '';
	const left = at - nowMs;
	if (left <= 0) return expiredText();
	if (left < 60000) return t('agents.helpers.confirmation.lessThanMinute');
	return t('agents.helpers.confirmation.expiresIn', {
		n: Math.ceil(left / 60000),
	});
};

export const safeResultUrl = (
	result: Record<string, unknown> | null,
): string | null => {
	const url = result?.url;
	if (typeof url !== 'string') return null;
	return /^https?:\/\//i.test(url) || /^\/(?![/\\])/.test(url) ? url : null;
};

export interface ActionOutcome {
	text: string;
	tone: 'ok' | 'error' | 'muted';
	url?: string | null;
}

export const actionOutcome = (
	a: PendingAction,
	nowMs: number = Date.now(),
): ActionOutcome | null => {
	switch (a.status) {
		case 'pending':
			return isPastExpiry(a, nowMs)
				? { text: expiredText(), tone: 'muted' }
				: null;
		case 'executing':
			return { text: t('agents.helpers.confirmation.applying'), tone: 'muted' };
		case 'executed':
			return {
				text: t('agents.helpers.confirmation.done'),
				tone: 'ok',
				url: safeResultUrl(a.result),
			};
		case 'failed': {
			const message = a.result?.message;
			return {
				text:
					typeof message === 'string' && message
						? t('agents.helpers.confirmation.failedWith', { message })
						: t('agents.helpers.confirmation.failed'),
				tone: 'error',
			};
		}
		case 'cancelled':
			return {
				text: t('agents.helpers.confirmation.cancelled'),
				tone: 'muted',
			};
		case 'expired':
			return { text: expiredText(), tone: 'muted' };
		case 'superseded':
			return {
				text: t('agents.helpers.confirmation.superseded'),
				tone: 'muted',
			};
	}
};

export const isLongBody = (body: string | null): boolean =>
	!!body && (body.length > 200 || body.split('\n').length > 3);

export const decisionConflictMessage = (): string =>
	t('agents.helpers.confirmation.conflict');
