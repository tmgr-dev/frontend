interface CategorySettingLike {
	key?: string;
	value?: string;
}

interface CategoryLike {
	settings?: CategorySettingLike[] | null;
}

interface TaskLike {
	id?: number | null;
	title?: string | null;
	category?: number | { code?: string | null } | null;
	category_tasks_sequence_id?: number | null;
}

const TITLE_PATTERN_SETTING = 'task_name_pattern_date&time';

/** "TM-{index#category}:" is how a category stamps every task title; "TM" is the key prefix. */
export const keyPrefixFromPattern = (pattern: string): string | null => {
	if (!pattern) {
		return null;
	}
	const match = /^([A-Za-z][A-Za-z0-9]{0,9})-\{/.exec(pattern.trim());
	return match ? match[1].toUpperCase() : null;
};

/** Every prefix used in this workspace, so only real keys become links. */
export const taskKeyPrefixes = (categories: CategoryLike[]): string[] => {
	const seen = new Set<string>();
	for (const category of categories || []) {
		const setting = (category?.settings || []).find(
			(s) => s?.key === TITLE_PATTERN_SETTING,
		);
		const prefix = setting?.value ? keyPrefixFromPattern(setting.value) : null;
		if (prefix) {
			seen.add(prefix);
		}
	}
	return [...seen];
};

const KEY_LIKE_PREFIX = /^\s*[A-Za-z][A-Za-z0-9]*-\d+(?=[:\s]|$)/;

/**
 * Whether a task answers to a key: its title starts with the key, or — only
 * when the title starts with no key at all — its category code and ticket
 * number spell it. A title stamped with another key keeps that key.
 */
export const taskCarriesKey = (
	title: string | null | undefined,
	code: string | null | undefined,
	sequence: number | null | undefined,
	key: string,
): boolean => {
	const wanted = (key || '').trim().toUpperCase();
	if (!wanted) {
		return false;
	}
	const head = (title || '').trimStart().toUpperCase();
	if (head.startsWith(wanted)) {
		const next = head.charAt(wanted.length);
		if (next === '' || next === ':' || /\s/.test(next)) {
			return true;
		}
	}
	if (KEY_LIKE_PREFIX.test(title || '')) {
		return false;
	}
	return (
		!!code && sequence != null && `${code}-${sequence}`.toUpperCase() === wanted
	);
};

/** The one item that carries the key; none or several mean the key names nothing. */
export const onlyCarrier = <T>(
	items: T[],
	carries: (item: T) => boolean,
): T | null => {
	const hits = (items || []).filter(carries);
	return hits.length === 1 ? hits[0] : null;
};

/**
 * The task a key names, among search hits. A hit that merely mentions the key,
 * or whose number merely starts with it, is not the task the reader asked for.
 */
export const pickTaskByKey = <T extends TaskLike>(
	tasks: T[],
	key: string,
): T | null =>
	onlyCarrier(tasks, (task) =>
		taskCarriesKey(
			task?.title,
			typeof task?.category === 'object' ? task.category?.code : null,
			task?.category_tasks_sequence_id,
			key,
		),
	);
