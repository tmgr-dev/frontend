/**
 * Text for a Recent Activity row (TM-228). The feed rendered whatever the API sent, so a row
 * whose `title` or `subject_name` came back empty showed as a blank line with a timestamp. These
 * helpers always produce something a person can read, and say plainly when there is nothing to
 * add rather than leaving an empty element behind.
 */

import { formatDistance } from 'date-fns';

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

const TITLES: Record<string, string> = {
	task_created: 'Created a task',
	task_updated: 'Updated a task',
	task_completed: 'Completed a task',
	task_deleted: 'Deleted a task',
	task_restored: 'Restored a task',
	task_status_changed: 'Moved a task',
	task_assigned: 'Assigned a task',
	task_persona_assigned: 'Assigned a task to a persona',
	task_persona_unassigned: 'Unassigned a task from a persona',
	task_timer_started: 'Started a timer',
	task_timer_stopped: 'Stopped a timer',
	comment_created: 'Wrote a comment',
	comment_updated: 'Edited a comment',
	comment_deleted: 'Deleted a comment',
	category_created: 'Created a category',
	category_updated: 'Updated a category',
	category_deleted: 'Deleted a category',
	file_uploaded: 'Attached a file',
	file_deleted: 'Removed a file',
	member_joined: 'Joined the workspace',
	member_left: 'Left the workspace',
	routine_completed: 'Completed a routine',
	agent_work_started: 'Started agent work',
	agent_work_updated: 'Updated agent work',
	agent_work_finished: 'Finished agent work',
};

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
		return `Assigned a task to ${persona}`;
	if (persona && type === 'task_persona_unassigned')
		return `Unassigned a task from ${persona}`;
	return TITLES[type] || fromType(type) || 'Activity';
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
	return formatDistance(at, now, { addSuffix: true })
		.replace('less than a minute ago', 'just now')
		.replace(/^about /, '');
}
