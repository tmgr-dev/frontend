import { presetTarget, previewSurfaces } from '../previewSurfaces';

const catalog: any[] = [
	{ key: 'categories', core: true, enabled: true },
	{ key: 'board', core: true, enabled: true },
	{ key: 'task.countdown', core: true, enabled: true },
	{ key: 'task.comments', core: true, enabled: true },
	{ key: 'task.assignees', core: true, enabled: true },
	{ key: 'dashboard', enabled: true },
	{ key: 'pomodoro', enabled: false },
	{ key: 'task.checkpoints', enabled: true },
	{ key: 'task.files', enabled: true },
	{ key: 'task.relations', enabled: false },
	{ key: 'daily_routines', enabled: false },
	{ key: 'pages', enabled: false },
	{ key: 'graph', enabled: false },
	{ key: 'ai.assistant', enabled: false },
	{ key: 'agent_work', enabled: false },
	{ key: 'personas', enabled: false },
	{ key: 'cursor', enabled: false },
	{ key: 'github', enabled: false },
	{ key: 'mcp', scope: 'user', enabled: true },
];
const preset = (key: string, modules: string[]) => ({
	key,
	name: key,
	description: '',
	modules,
});
const byId = (items: { id: string; state: string }[]) =>
	Object.fromEntries(items.map((i) => [i.id, i.state]));

describe('presetTarget', () => {
	it('keeps core on and follows the preset list otherwise', () => {
		const on = presetTarget(preset('personal', ['dashboard']), catalog);
		expect(on('board')).toBe(true);
		expect(on('dashboard')).toBe(true);
		expect(on('pages')).toBe(false);
	});
	it('turns every workspace module on for everything', () => {
		const on = presetTarget(preset('everything', []), catalog);
		expect(on('pages')).toBe(true);
		expect(on('cursor')).toBe(true);
	});
});

describe('previewSurfaces', () => {
	const personal = presetTarget(
		preset('personal', [
			'dashboard',
			'task.checkpoints',
			'task.files',
			'pomodoro',
		]),
		catalog,
	);

	it('marks nav and task items on or off from the preset', () => {
		const { nav, task } = previewSurfaces(personal);
		expect(byId(nav)).toMatchObject({
			dashboard: 'on',
			list: 'on',
			board: 'on',
			categories: 'on',
			files: 'on',
			pages: 'off',
			map: 'off',
			routines: 'off',
			'ask-ai': 'off',
			personas: 'off',
		});
		expect(byId(task)).toMatchObject({
			timer: 'on',
			comments: 'on',
			assignees: 'on',
			pomodoro: 'on',
			checkpoints: 'on',
			attachments: 'on',
			relations: 'off',
			'ai-reply': 'off',
			'agent-work': 'off',
			github: 'off',
			cursor: 'off',
		});
	});

	it('shows developer items', () => {
		const dev = presetTarget(
			preset('developer', [
				'ai.assistant',
				'personas',
				'cursor',
				'github',
				'agent_work',
			]),
			catalog,
		);
		const { nav, task } = previewSurfaces(dev);
		expect(byId(nav)['ask-ai']).toBe('on');
		expect(byId(nav).personas).toBe('on');
		expect(byId(task)['ai-reply']).toBe('on');
		expect(byId(task).github).toBe('on');
	});

	it('marks new and will-hide against the current state', () => {
		const current = (key: string) =>
			catalog.find((m) => m.key === key)?.enabled === true;
		const { nav, task } = previewSurfaces(personal, current);
		expect(byId(task).pomodoro).toBe('new');
		expect(byId(task).checkpoints).toBe('on');
		expect(byId(nav).dashboard).toBe('on');
		const dev = previewSurfaces(
			presetTarget(preset('developer', ['ai.assistant']), catalog),
			current,
		);
		expect(byId(dev.nav).dashboard).toBe('will-hide');
		expect(byId(dev.nav)['ask-ai']).toBe('new');
		expect(byId(dev.task).attachments).toBe('will-hide');
	});
});

describe('wiring', () => {
	const { readFileSync } = require('fs');
	const { join } = require('path');
	const read = (file: string) =>
		readFileSync(join(__dirname, '../../', file), 'utf8');

	it('shares options and preview between the picker and the dialog', () => {
		for (const file of [
			'components/general/ModulesPicker.vue',
			'components/general/ModulesPresetDialog.vue',
		]) {
			expect(read(file)).toContain('<ModulePresetOptions');
			expect(read(file)).toContain('<ModulePresetPreview');
		}
	});
	it('exposes the test ids', () => {
		const all = [
			'components/general/ModulePresetPreview.vue',
			'components/general/ModulePresetOptions.vue',
			'components/general/ModulesPresetDialog.vue',
			'pages/Settings/Modules.vue',
		]
			.map(read)
			.join('\n');
		for (const id of [
			'preset-preview',
			'preview-nav-',
			'preview-task-',
			'preset-option-',
			'preset-diff-on',
			'preset-diff-off',
			'preset-confirm',
			'apply-preset',
		])
			expect(all).toContain(id);
	});
	it('disables the apply button for non-owners', () => {
		expect(read('pages/Settings/Modules.vue')).toContain(
			':disabled="!canApplyPreset(isOwner)"',
		);
	});
	it('applies a preset with only the preset key', () => {
		expect(read('components/general/ModulesPresetDialog.vue')).toContain(
			'buildChoiceRequest(selected.value.key, {})',
		);
	});
});
