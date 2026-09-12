interface CategorySettingLike {
	key?: string;
	value?: string;
}

interface CategoryLike {
	settings?: CategorySettingLike[] | null;
}

interface TaskLike {
	id: number;
	title?: string | null;
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
		const setting = (category?.settings || []).find((s) => s?.key === TITLE_PATTERN_SETTING);
		const prefix = setting?.value ? keyPrefixFromPattern(setting.value) : null;
		if (prefix) {
			seen.add(prefix);
		}
	}
	return [...seen];
};

/**
 * The task a key names. The key lives in the title prefix ("TM-129: …"), so a
 * search hit that merely mentions the key, or one whose number merely starts
 * with it, is not the task the reader asked for.
 */
export const pickTaskByKey = <T extends TaskLike>(tasks: T[], key: string): T | null => {
	const wanted = (key || '').trim().toUpperCase();
	if (!wanted) {
		return null;
	}
	const found = (tasks || []).find((task) => {
		const title = (task?.title || '').trim().toUpperCase();
		return title.startsWith(`${wanted}:`) || title.startsWith(`${wanted} :`);
	});
	return found ?? null;
};
