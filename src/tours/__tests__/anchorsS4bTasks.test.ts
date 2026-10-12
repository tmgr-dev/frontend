import { readFileSync } from 'fs';
import { join } from 'path';
import { selectionStarted } from '../events';

const ANCHORS: Record<string, string[]> = {
	'pages/NewForm.vue': ['task-form.pomodoro', 'task-form.checkpoints'],
	'components/SettingsComponent.vue': ['task-form.estimate'],
	'components/tasks/PomodoroBlock.vue': ['pomodoro.settings'],
	'components/tasks/TaskRelations.vue': ['task-form.relations'],
	'pages/TasksListPage.vue': ['list.select'],
	'components/tasks/TasksListComponent.vue': ['list.process'],
	'components/tasks/TasksMultipleActionsModal.vue': ['export.panel', 'export.download'],
	'components/general/FiltersBoard.vue': ['statuses.editor'],
	'pages/ProjectCategoryList.vue': ['categories.create', 'categories.row'],
	'pages/WorkspaceFilesPage.vue': ['files.list'],
};

const read = (file: string) => readFileSync(join(__dirname, '../..', file), 'utf8');

describe('S4b task-area data-tour anchors', () => {
	it.each(Object.entries(ANCHORS).flatMap(([file, list]) => list.map((a) => [file, a])))(
		'%s has %s',
		(file, anchor) => {
			expect(read(file)).toMatch(new RegExp(`data-tour="${anchor.replace(/\./g, '\\.')}"`));
		},
	);
});

describe('TasksMultipleActionsModal', () => {
	it('keeps exportsEnabled in the only computed block so the export panel can render', () => {
		const source = read('components/tasks/TasksMultipleActionsModal.vue');
		expect(source.match(/^\t\tcomputed: \{/gm)).toHaveLength(1);
		expect(source).toMatch(/computed: \{\s*exportsEnabled\(\)/);
	});
});

describe('S4b task-area emits', () => {
	it('NewForm emits checkpoint.added from addCheckpoint', () => {
		expect(read('pages/NewForm.vue')).toMatch(/tourBus\.emit\('checkpoint\.added'/);
	});

	it('TasksListComponent emits tasks.selected and tasks.actions_opened', () => {
		const source = read('components/tasks/TasksListComponent.vue');
		expect(source).toMatch(/tourBus\.emit\('tasks\.selected'/);
		expect(source).toMatch(/showProcessModal\(\) \{\s*tourBus\.emit\('tasks\.actions_opened'\)/);
	});
});

describe('selectionStarted', () => {
	it.each([
		[0, 1, true],
		[0, 5, true],
		[1, 2, false],
		[2, 1, false],
		[1, 0, false],
		[0, 0, false],
	])('%i -> %i is %s', (prev, next, expected) => {
		expect(selectionStarted(prev, next)).toBe(expected);
	});
});
