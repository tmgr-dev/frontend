/**
 * Text for a Recent Activity row (TM-228). The feed rendered whatever the API sent, so a row
 * whose `title` or `subject_name` came back empty showed as a blank line with a timestamp. These
 * helpers always produce something a person can read, and say plainly when there is nothing to
 * add rather than leaving an empty element behind.
 */

export interface ActivityLike {
	type?: string | null;
	title?: string | null;
	subject_name?: string | null;
	subject_type?: string | null;
	subject_id?: number | null;
	metadata?: Record<string, unknown> | null;
}

const TITLES: Record<string, string> = {
	task_created: 'Created a task',
	task_updated: 'Updated a task',
	task_completed: 'Completed a task',
	task_deleted: 'Deleted a task',
	task_restored: 'Restored a task',
	task_status_changed: 'Moved a task',
	task_assigned: 'Assigned a task',
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
};

const text = (value: unknown): string => (typeof value === 'string' ? value.trim() : '');

/** Humanises an unknown event key: `invoice_paid` → `Invoice paid`. */
function fromType(type: string): string {
	const words = type.replace(/[_-]+/g, ' ').trim();
	return words ? words.charAt(0).toUpperCase() + words.slice(1) : '';
}

export function activityTitle(activity: ActivityLike): string {
	const given = text(activity.title);
	if (given) return given;

	const type = text(activity.type);
	return TITLES[type] || fromType(type) || 'Activity';
}

export function activitySubject(activity: ActivityLike): string {
	const given = text(activity.subject_name);
	if (given) return given;

	const fromMetadata = text(activity.metadata?.task_title);
	if (fromMetadata) return fromMetadata;

	const type = text(activity.subject_type).split('\\').pop() ?? '';
	return type && activity.subject_id ? `${type} #${activity.subject_id}` : '';
}
