import { t } from '@/i18n';
import type { ModuleEntry, ModulePreset } from '@/utils/modules';

export type PreviewState = 'on' | 'off' | 'new' | 'will-hide';

export interface PreviewItem {
	id: string;
	label: string;
	state: PreviewState;
}

type Spec = { id: string; labelKey: string; key: string | null };

const NAV: Spec[] = [
	{
		id: 'dashboard',
		labelKey: 'general.helpers.previewSurfaces.nav.dashboard',
		key: 'dashboard',
	},
	{
		id: 'list',
		labelKey: 'general.helpers.previewSurfaces.nav.list',
		key: null,
	},
	{
		id: 'board',
		labelKey: 'general.helpers.previewSurfaces.nav.board',
		key: 'board',
	},
	{
		id: 'routines',
		labelKey: 'general.helpers.previewSurfaces.nav.routines',
		key: 'daily_routines',
	},
	{
		id: 'categories',
		labelKey: 'general.helpers.previewSurfaces.nav.projects',
		key: 'categories',
	},
	{
		id: 'pages',
		labelKey: 'general.helpers.previewSurfaces.nav.pages',
		key: 'pages',
	},
	{
		id: 'map',
		labelKey: 'general.helpers.previewSurfaces.nav.map',
		key: 'graph',
	},
	{
		id: 'files',
		labelKey: 'general.helpers.previewSurfaces.nav.files',
		key: 'task.files',
	},
	{
		id: 'ask-ai',
		labelKey: 'general.helpers.previewSurfaces.nav.askAi',
		key: 'ai.assistant',
	},
	{
		id: 'personas',
		labelKey: 'general.helpers.previewSurfaces.nav.personas',
		key: 'personas',
	},
];

const TASK: Spec[] = [
	{
		id: 'timer',
		labelKey: 'general.helpers.previewSurfaces.task.timer',
		key: 'task.countdown',
	},
	{
		id: 'pomodoro',
		labelKey: 'general.helpers.previewSurfaces.task.pomodoro',
		key: 'pomodoro',
	},
	{
		id: 'assignees',
		labelKey: 'general.helpers.previewSurfaces.task.assignees',
		key: 'task.assignees',
	},
	{
		id: 'checkpoints',
		labelKey: 'general.helpers.previewSurfaces.task.checkpoints',
		key: 'task.checkpoints',
	},
	{
		id: 'attachments',
		labelKey: 'general.helpers.previewSurfaces.task.attachments',
		key: 'task.files',
	},
	{
		id: 'relations',
		labelKey: 'general.helpers.previewSurfaces.task.relations',
		key: 'task.relations',
	},
	{
		id: 'agent-work',
		labelKey: 'general.helpers.previewSurfaces.task.agentWork',
		key: 'agent_work',
	},
	{
		id: 'github',
		labelKey: 'general.helpers.previewSurfaces.task.github',
		key: 'github',
	},
	{
		id: 'cursor',
		labelKey: 'general.helpers.previewSurfaces.task.cursorAgents',
		key: 'cursor',
	},
	{
		id: 'comments',
		labelKey: 'general.helpers.previewSurfaces.task.comments',
		key: 'task.comments',
	},
	{
		id: 'ai-reply',
		labelKey: 'general.helpers.previewSurfaces.task.aiReply',
		key: 'ai.assistant',
	},
];

export const presetTarget = (
	preset: ModulePreset,
	catalog: Pick<ModuleEntry, 'key' | 'core'>[],
) => {
	const core = new Set(catalog.filter((m) => m.core).map((m) => m.key));
	const known = new Set(catalog.map((m) => m.key));
	return (key: string): boolean =>
		core.has(key) ||
		(key === 'workflow' && !known.has(key)) ||
		preset.key === 'everything' ||
		preset.modules.includes(key);
};

export const currentTarget =
	(catalog: Pick<ModuleEntry, 'key' | 'core' | 'enabled'>[]) =>
	(key: string): boolean => {
		const entry = catalog.find((m) => m.key === key);
		if (!entry) return key === 'workflow';
		return entry.enabled === true || entry.core === true;
	};

export const previewSurfaces = (
	target: (key: string) => boolean,
	current?: (key: string) => boolean,
): { nav: PreviewItem[]; task: PreviewItem[]; card: 'status' | 'done' } => {
	const active = (check: (key: string) => boolean, key: string | null) =>
		key === null || (check(key) && (key !== 'board' || check('workflow')));
	const build = (specs: Spec[]): PreviewItem[] =>
		specs.map(({ id, labelKey, key }) => {
			const next = active(target, key);
			const was = !current || active(current, key);
			const state: PreviewState =
				next && !was
					? 'new'
					: !next && was && current
						? 'will-hide'
						: next
							? 'on'
							: 'off';
			return { id, label: t(labelKey), state };
		});
	return {
		nav: build(NAV),
		task: build(TASK),
		card: target('workflow') ? 'status' : 'done',
	};
};
