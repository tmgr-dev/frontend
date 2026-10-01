import type { ParsedTmgrUrl } from './tmgrLinks';

export interface TaskLinkRef {
	kind: 'page' | 'task';
	id: string;
}

export const extractTaskLinkRefs = (
	...sources: (string | null | undefined | object)[]
): TaskLinkRef[] => {
	const seen = new Set<string>();
	const refs: TaskLinkRef[] = [];
	for (const source of sources) {
		if (!source) continue;
		const text = typeof source === 'string' ? source : JSON.stringify(source);
		for (const match of text.matchAll(/tmgr:\/\/(page|task)\/(\d+)/g)) {
			const key = `${match[1]}/${match[2]}`;
			if (seen.has(key)) continue;
			seen.add(key);
			refs.push({ kind: match[1] as 'page' | 'task', id: match[2] });
		}
	}
	return refs;
};

export const routeForTmgr = (
	parsed: ParsedTmgrUrl,
	workspaceCode: string,
): string | null => {
	if (parsed.form === 'deep') {
		return `/${parsed.workspace}/pages/${parsed.slug}`;
	}
	switch (parsed.kind) {
		case 'page':
			return `/${workspaceCode}/pages/${parsed.id}`;
		case 'task':
			return `/${workspaceCode}/tasks/${parsed.id}`;
		case 'user':
			return `/${workspaceCode}/team/${parsed.id}`;
		default:
			return null;
	}
};
