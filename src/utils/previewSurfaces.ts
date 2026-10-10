import type { ModuleEntry, ModulePreset } from '@/utils/modules';

export type PreviewState = 'on' | 'off' | 'new' | 'will-hide';

export interface PreviewItem {
	id: string;
	label: string;
	state: PreviewState;
}

type Spec = { id: string; label: string; key: string | null };

const NAV: Spec[] = [
	{ id: 'dashboard', label: 'Dashboard', key: 'dashboard' },
	{ id: 'list', label: 'List', key: null },
	{ id: 'board', label: 'Board', key: 'board' },
	{ id: 'routines', label: 'Routines', key: 'daily_routines' },
	{ id: 'categories', label: 'Projects', key: 'categories' },
	{ id: 'pages', label: 'Pages', key: 'pages' },
	{ id: 'map', label: 'Map', key: 'graph' },
	{ id: 'files', label: 'Files', key: 'task.files' },
	{ id: 'ask-ai', label: 'Ask AI', key: 'ai.assistant' },
	{ id: 'personas', label: 'Personas', key: 'personas' },
];

const TASK: Spec[] = [
	{ id: 'timer', label: 'Timer', key: 'task.countdown' },
	{ id: 'pomodoro', label: 'Pomodoro', key: 'pomodoro' },
	{ id: 'assignees', label: 'Assignees', key: 'task.assignees' },
	{ id: 'checkpoints', label: 'Checkpoints', key: 'task.checkpoints' },
	{ id: 'attachments', label: 'Attachments', key: 'task.files' },
	{ id: 'relations', label: 'Relations', key: 'task.relations' },
	{ id: 'agent-work', label: 'Agent work', key: 'agent_work' },
	{ id: 'github', label: 'GitHub', key: 'github' },
	{ id: 'cursor', label: 'Cursor agents', key: 'cursor' },
	{ id: 'comments', label: 'Comments', key: 'task.comments' },
	{ id: 'ai-reply', label: 'AI reply', key: 'ai.assistant' },
];

export const presetTarget = (
	preset: ModulePreset,
	catalog: Pick<ModuleEntry, 'key' | 'core'>[],
) => {
	const core = new Set(catalog.filter((m) => m.core).map((m) => m.key));
	return (key: string): boolean =>
		core.has(key) ||
		preset.key === 'everything' ||
		preset.modules.includes(key);
};

export const previewSurfaces = (
	target: (key: string) => boolean,
	current?: (key: string) => boolean,
): { nav: PreviewItem[]; task: PreviewItem[] } => {
	const build = (specs: Spec[]): PreviewItem[] =>
		specs.map(({ id, label, key }) => {
			const next = key === null || target(key);
			const was = key === null || !current || current(key);
			const state: PreviewState =
				next && !was
					? 'new'
					: !next && was && current
					? 'will-hide'
					: next
					? 'on'
					: 'off';
			return { id, label, state };
		});
	return { nav: build(NAV), task: build(TASK) };
};
