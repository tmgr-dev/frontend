/**
 * Text for a Recent Activity row (TM-228). The feed rendered whatever the API sent, so a row
 * whose `title` or `subject_name` came back empty showed as a blank line with a timestamp. These
 * helpers always produce something a person can read, and say plainly when there is nothing to
 * add rather than leaving an empty element behind.
 */

import { currentLocale, t } from '@/i18n';

export interface ActivityLike {
	type?: string | null;
	title?: string | null;
	subject_name?: string | null;
	subject_type?: string | null;
	subject_id?: number | null;
	metadata?: Record<string, unknown> | string | null;
	timestamp_human?: string | null;
	created_at?: string | null;
}

const TITLE_TYPES = [
	'task_created',
	'task_updated',
	'task_completed',
	'task_deleted',
	'task_restored',
	'task_status_changed',
	'task_assigned',
	'task_persona_assigned',
	'task_persona_unassigned',
	'task_timer_started',
	'task_timer_stopped',
	'comment_created',
	'comment_updated',
	'comment_deleted',
	'category_created',
	'category_updated',
	'category_deleted',
	'file_uploaded',
	'file_deleted',
	'member_joined',
	'member_left',
	'routine_completed',
	'agent_work_started',
	'agent_work_updated',
	'agent_work_finished',
];

const camel = (type: string) =>
	type.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase());

const knownTitle = (type: string): string =>
	TITLE_TYPES.includes(type)
		? t(`board.helpers.activityTitles.${camel(type)}`)
		: '';

const metadataOf = (activity: ActivityLike): Record<string, unknown> => {
	const raw = activity.metadata;
	if (raw && typeof raw === 'object') return raw;
	if (typeof raw !== 'string' || !raw) return {};
	try {
		const parsed = JSON.parse(raw);
		return parsed && typeof parsed === 'object' ? parsed : {};
	} catch {
		return {};
	}
};

const text = (value: unknown): string =>
	typeof value === 'string' ? value.trim() : '';

/** Humanises an unknown event key: `invoice_paid` → `Invoice paid`. */
function fromType(type: string): string {
	const words = type.replace(/[_-]+/g, ' ').trim();
	return words ? words.charAt(0).toUpperCase() + words.slice(1) : '';
}

export function activityTitle(activity: ActivityLike): string {
	const given = text(activity.title);
	if (given) return given;

	const type = text(activity.type);
	const persona = text(metadataOf(activity).persona_name);
	if (persona && type === 'task_persona_assigned')
		return t('board.helpers.activityTitles.assignedToPersona', { persona });
	if (persona && type === 'task_persona_unassigned')
		return t('board.helpers.activityTitles.unassignedFromPersona', {
			persona,
		});
	return (
		knownTitle(type) ||
		fromType(type) ||
		t('board.helpers.activityTitles.fallback')
	);
}

export function activitySubject(activity: ActivityLike): string {
	const given = text(activity.subject_name);
	if (given) return given;

	const fromMetadata = text(metadataOf(activity).task_title);
	if (fromMetadata) return fromMetadata;

	const type = text(activity.subject_type).split('\\').pop() ?? '';
	return type && activity.subject_id ? `${type} #${activity.subject_id}` : '';
}

export function activityTime(
	activity: ActivityLike,
	now: number = Date.now(),
): string {
	const given = text(activity.timestamp_human);
	if (given) return given;
	const created = text(activity.created_at);
	const at = created ? Date.parse(created) : NaN;
	if (Number.isNaN(at)) return '';
	const seconds = Math.round((at - now) / 1000);
	const abs = Math.abs(seconds);
	if (abs < 60) return t('tasks.helpers.relativeTime.justNow');
	const units: [Intl.RelativeTimeFormatUnit, number][] = [
		['year', 31536000],
		['month', 2592000],
		['day', 86400],
		['hour', 3600],
		['minute', 60],
	];
	const [unit, size] = units.find(([, s]) => abs >= s)!;
	return new Intl.RelativeTimeFormat(currentLocale(), {
		numeric: 'auto',
	}).format(Math.trunc(seconds / size), unit);
}
