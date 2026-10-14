import { titlePatternHandler } from '@/utils/titlePatternHandler';

export const TITLE_PATTERN_KEY = 'task_name_pattern_date&time';
export const RATE_KEY = 'task_cost_per_hour';
export const ESTIMATE_KEY = 'approximately_time';
export const REMINDER_KEY = 'active_countdown_reminder_every';

export interface CategorySettingValue {
	id: number;
	key?: string;
	value?: string | number | null;
}

export interface CategoryLike {
	id: number;
	title: string;
	project_category_id?: number | null;
	deleted_at?: string | null;
	settings?: CategorySettingValue[] | null;
}

export interface CategoryNode<T extends CategoryLike = CategoryLike> {
	category: T;
	depth: number;
	ancestors: T[];
}

export interface CategoryGroup<T extends CategoryLike = CategoryLike> {
	root: T;
	nodes: CategoryNode<T>[];
}

const byTitle = (a: CategoryLike, b: CategoryLike) =>
	a.title.localeCompare(b.title, undefined, { sensitivity: 'base' }) ||
	a.id - b.id;

export const activeCategories = <T extends CategoryLike>(categories: T[]) =>
	(categories || []).filter((c) => !c.deleted_at);

export const buildCategoryGroups = <T extends CategoryLike>(
	categories: T[],
): CategoryGroup<T>[] => {
	const list = activeCategories(categories);
	const ids = new Set(list.map((c) => c.id));
	const children = new Map<number | null, T[]>();
	for (const c of list) {
		const parent =
			c.project_category_id != null && ids.has(c.project_category_id)
				? c.project_category_id
				: null;
		children.set(parent, [...(children.get(parent) || []), c]);
	}
	const seen = new Set<number>();
	const walk = (parent: T, depth: number, ancestors: T[], out: CategoryNode<T>[]) => {
		seen.add(parent.id);
		out.push({ category: parent, depth, ancestors });
		for (const child of [...(children.get(parent.id) || [])].sort(byTitle)) {
			if (!seen.has(child.id)) walk(child, depth + 1, [...ancestors, parent], out);
		}
	};
	const groups = [...(children.get(null) || [])].sort(byTitle).map((root) => {
		const nodes: CategoryNode<T>[] = [];
		walk(root, 0, [], nodes);
		return { root, nodes };
	});
	const cyclic = list.filter((c) => !seen.has(c.id)).sort(byTitle);
	for (const c of cyclic) {
		if (seen.has(c.id)) continue;
		const nodes: CategoryNode<T>[] = [];
		walk(c, 0, [], nodes);
		groups.push({ root: c, nodes });
	}
	return groups;
};

export const flattenGroups = <T extends CategoryLike>(groups: CategoryGroup<T>[]) =>
	groups.flatMap((g) => g.nodes);

export const descendantIds = (categories: CategoryLike[], id: number) => {
	const result = new Set<number>();
	const queue = [id];
	while (queue.length) {
		const current = queue.shift();
		for (const c of categories) {
			if (c.project_category_id === current && !result.has(c.id) && c.id !== id) {
				result.add(c.id);
				queue.push(c.id);
			}
		}
	}
	return result;
};

export const parentOptions = <T extends CategoryLike>(
	nodes: CategoryNode<T>[],
	selfId: number | null,
) => {
	if (selfId == null) return nodes;
	const excluded = descendantIds(
		nodes.map((n) => n.category),
		selfId,
	);
	excluded.add(selfId);
	return nodes.filter((n) => !excluded.has(n.category.id));
};

export const settingValue = (category: CategoryLike | null | undefined, key: string) => {
	const value = category?.settings?.find((s) => s?.key === key)?.value;
	return value == null ? '' : String(value);
};

export const isLiteralPattern = (pattern: string | null | undefined) =>
	!!pattern && !pattern.includes('{');

export const literalPatternFix = (pattern: string) =>
	`{dts#${pattern.toLowerCase()}#dte}`;

export const renderTitlePrefix = (pattern: string | null | undefined) => {
	if (!pattern) return '';
	try {
		return titlePatternHandler(pattern.replace(/\{index#[a-z]+\}/gi, '#'));
	} catch {
		return pattern;
	}
};

export const composeTitle = (prefix: string, title: string) => {
	if (!prefix) return title;
	return prefix.endsWith(':') ? `${prefix} ${title}` : `${prefix}: ${title}`;
};

export const formatDuration = (value: string | number | null | undefined) => {
	const seconds = Number(value);
	if (!Number.isFinite(seconds) || seconds <= 0) return '';
	const hours = Math.floor(seconds / 3600);
	const minutes = Math.floor((seconds % 3600) / 60);
	if (!hours) return `${minutes}m`;
	return minutes ? `${hours}h ${minutes}m` : `${hours}h`;
};

export const categoryMeta = (category: CategoryLike) => {
	const rate = settingValue(category, RATE_KEY);
	return [
		renderTitlePrefix(settingValue(category, TITLE_PATTERN_KEY)),
		rate && Number(rate) !== 0 ? `${rate}/h` : '',
		formatDuration(settingValue(category, ESTIMATE_KEY)),
	]
		.filter(Boolean)
		.join(' · ');
};

export const crumbOf = (ancestors: CategoryLike[], workspaceName: string) =>
	ancestors.length
		? `${ancestors.map((a) => a.title).join(' / ')} /`
		: `${workspaceName} · top level`;

export const literalIssues = <T extends CategoryLike>(nodes: CategoryNode<T>[]) =>
	nodes
		.filter((n) => isLiteralPattern(settingValue(n.category, TITLE_PATTERN_KEY)))
		.map((n) => ({
			category: n.category,
			path: n.ancestors.map((a) => a.title).join(' / '),
			pattern: settingValue(n.category, TITLE_PATTERN_KEY),
		}));

export const hasSettings = (category: CategoryLike) =>
	(category.settings || []).some((s) => s?.value != null && s.value !== '');

export const pickCopySource = <T extends CategoryLike>(
	nodes: CategoryNode<T>[],
	parentId: number | null,
) =>
	nodes.find(
		(n) =>
			(n.ancestors[n.ancestors.length - 1]?.id ?? null) === parentId &&
			hasSettings(n.category) &&
			!isLiteralPattern(settingValue(n.category, TITLE_PATTERN_KEY)),
	)?.category ?? null;

export const fixedSettingsPayload = (category: CategoryLike) =>
	(category.settings || []).map((s) => ({
		id: s.id,
		value:
			s.key === TITLE_PATTERN_KEY && isLiteralPattern(String(s.value ?? ''))
				? literalPatternFix(String(s.value))
				: s.value,
	}));

export const sanitizeCategoryCode = (code: string, final = false) => {
	const cleaned = (code || '')
		.toUpperCase()
		.replace(/[^A-Z0-9-]/g, '-')
		.replace(/-+/g, '-')
		.replace(/^-+/, '');
	return final ? cleaned.replace(/-+$/, '') : cleaned;
};

export const insertAt = (value: string, token: string, start: number, end = start) => {
	const from = Math.max(0, Math.min(start, value.length));
	const to = Math.max(from, Math.min(end, value.length));
	return {
		value: value.slice(0, from) + token + value.slice(to),
		caret: from + token.length,
	};
};
