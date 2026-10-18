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

// Only ASCII spaces around the key: the local SQL candidate filter in
// resolveTaskKey selects every title this accepts, so both must agree.
const TITLE_KEY = /^ *([A-Za-z][A-Za-z0-9]*-\d+)(?:( *:)|(?= |$))/;

/**
 * "TM-242: …" stamps any key; "TM-242 …" only a key in the task's own
 * category, so "UTF-8 encoding bug" is a title, not a key.
 */
const stampOf = (
	title: string | null | undefined,
	code: string | null | undefined,
): { key: string; length: number } | null => {
	const match = TITLE_KEY.exec(title || '');
	if (!match) return null;
	const key = match[1].toUpperCase();
	const own = (code || '').trim().toUpperCase();
	const accepted =
		!!match[2] || (!!own && key.slice(0, key.lastIndexOf('-')) === own);
	return accepted ? { key, length: match[0].length } : null;
};

/** The key a title is stamped with, upper-cased; without a code only "KEY:" counts. */
export const titleKey = (
	title: string | null | undefined,
	code?: string | null,
): string | null => stampOf(title, code)?.key ?? null;

/** The title without its key stamp: "TM-242: Fix login" reads "Fix login". */
export const withoutTitleKey = (
	title: string,
	code?: string | null,
): string => {
	const stamp = stampOf(title, code);
	return stamp ? title.slice(stamp.length).replace(/^[: ]+/, '') : title;
};

/**
 * A task's key: the one its title is stamped with, else its category code and
 * ticket number. A title stamped with another key keeps that key.
 */
export const taskKeyFrom = (
	title: string | null | undefined,
	code: string | null | undefined,
	sequence: number | null | undefined,
): string | null =>
	titleKey(title, code) ??
	(code && code.trim() && sequence != null ? `${code}-${sequence}` : null);

/** Whether a task answers to a key — exactly the key it is shown with. */
export const taskCarriesKey = (
	title: string | null | undefined,
	code: string | null | undefined,
	sequence: number | null | undefined,
	key: string,
): boolean => {
	const wanted = (key || '').trim().toUpperCase();
	return (
		!!wanted && taskKeyFrom(title, code, sequence)?.toUpperCase() === wanted
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
