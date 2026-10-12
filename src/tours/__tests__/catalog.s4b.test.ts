import { getTour } from '../catalog';

const tour = (id: string) => getTour(id)!;
const anchors = (id: string) => tour(id).steps.map((s) => s.anchor);
const waits = (id: string) => tour(id).steps.filter((s) => s.type === 'wait').map((s) => s.waitFor?.event);

const S4B_IDS = [
	'tasks.statuses',
	'tasks.categories',
	'tasks.relations',
	'tasks.files-page',
	'tasks.multi-select',
	'time.parallel',
	'time.checkpoints',
	'time.pomodoro',
	'time.export',
	'pages.first',
	'pages.context',
	'pages.links',
	'pages.versions',
	'pages.people-meetings',
	'pages.import',
	'pages.task-from-selection',
	'graph.map',
	'graph.enable',
	'routines.first',
	'routines.views',
	'routines.reminders',
];

const LIST = { name: 'WorkspaceTasksList', params: 'currentWorkspace' };
const PAGES = { name: 'WorkspacePages', params: 'currentWorkspace' };

describe('S4b catalog', () => {
	it('registers every S4b tour with the right group and a hub trigger', () => {
		for (const id of S4B_IDS) {
			expect(tour(id)).toBeDefined();
			expect(tour(id).group).toBe(id.split('.')[0] === 'tasks' ? 'tasks' : id.split('.')[0]);
			expect(tour(id).version).toBe(1);
			expect(tour(id).since).toBe('3.4.157');
			expect(tour(id).triggers).toEqual(expect.arrayContaining([{ kind: 'hub' }]));
		}
	});

	it('names every anchor of the S4b table', () => {
		expect(anchors('tasks.statuses')).toEqual(['statuses.editor', 'statuses.editor']);
		expect(anchors('tasks.categories')).toEqual(['categories.create', 'categories.row']);
		expect(anchors('tasks.relations')).toEqual(['task-form.relations']);
		expect(anchors('tasks.files-page')).toEqual(['files.list']);
		expect(anchors('tasks.multi-select')).toEqual(['list.select']);
		expect(anchors('time.timer')).toEqual(['task-form.timer', 'nav.active-timers', 'task-form.timer', 'task-form.estimate']);
		expect(anchors('time.parallel')).toEqual(['nav.active-timers']);
		expect(anchors('time.checkpoints')).toEqual(['task-form.checkpoints']);
		expect(anchors('time.pomodoro')).toEqual(['task-form.pomodoro', 'pomodoro.settings']);
		expect(anchors('time.export')).toEqual(['list.select', 'list.process', 'export.panel', 'export.download']);
		expect(anchors('pages.first')).toEqual(['pages.new', 'pages.tree']);
		expect(anchors('pages.context')).toEqual(['pages.type-picker', 'page.editor']);
		expect(anchors('pages.links')).toEqual(['page.editor', 'page.backlinks']);
		expect(anchors('pages.versions')).toEqual(['page.versions']);
		expect(anchors('pages.people-meetings')).toEqual(['pages.type-picker']);
		expect(anchors('pages.import')).toEqual(['pages.import', 'pages.export']);
		expect(anchors('pages.task-from-selection')).toEqual(['page.editor']);
		expect(anchors('graph.map')).toEqual(['map.canvas', 'map.filters', 'map.search']);
		expect(anchors('graph.enable')).toEqual(['modules.graph']);
		expect(anchors('routines.first')).toEqual(['routines.add', 'routines.instance']);
		expect(anchors('routines.views')).toEqual(['routines.view-switcher']);
		expect(anchors('routines.reminders')).toEqual(['routines.instance', 'routine.reminder', 'routine.to-task']);
	});

	it('wait steps that navigate carry no route', () => {
		for (const id of S4B_IDS)
			for (const step of tour(id).steps)
				if (step.type === 'wait' && step.takeMeRoute) expect(step.route).toBeUndefined();
	});

	it('tours with a tip after the last wait step do not complete on the wait event', () => {
		for (const id of S4B_IDS) {
			const { steps, completeOn } = tour(id);
			const lastWait = steps.map((s) => s.type).lastIndexOf('wait');
			if (lastWait >= 0 && lastWait < steps.length - 1) expect(`${id}: ${completeOn}`).toBe(`${id}: undefined`);
		}
	});

	it('cloud-only tours exclude desktop_local', () => {
		for (const id of S4B_IDS) {
			const { requires } = tour(id);
			if (requires?.account === 'cloud') expect(requires.platforms).not.toContain('desktop_local');
		}
	});

	it('platforms and requirements follow the plan', () => {
		const ALL = ['web', 'mobile_web', 'desktop', 'desktop_local'];
		const WDDL = ['web', 'desktop', 'desktop_local'];
		expect(tour('tasks.statuses').requires).toMatchObject({ modules: ['workflow', 'board'], role: 'owner', platforms: WDDL });
		expect(tour('tasks.categories').requires).toMatchObject({ modules: ['categories'], platforms: ALL });
		expect(tour('tasks.relations').requires).toMatchObject({ modules: ['task.relations'], platforms: WDDL });
		expect(tour('tasks.files-page').requires).toMatchObject({ modules: ['task.files'], platforms: ALL });
		expect(tour('tasks.multi-select').requires?.platforms).toEqual(WDDL);
		expect(tour('time.parallel').requires?.platforms).toEqual(WDDL);
		expect(tour('time.checkpoints').requires).toMatchObject({ modules: ['task.checkpoints'], platforms: WDDL });
		expect(tour('time.pomodoro').requires).toMatchObject({ modules: ['pomodoro'], account: 'cloud', platforms: ['web', 'mobile_web', 'desktop'] });
		expect(tour('time.export').requires).toMatchObject({ modules: ['exports'], account: 'cloud', platforms: ['web', 'desktop'] });
		expect(tour('pages.first').requires).toMatchObject({ modules: ['pages'], platforms: ALL });
		for (const id of ['pages.context', 'pages.links', 'pages.versions', 'pages.people-meetings', 'pages.import', 'pages.task-from-selection'])
			expect(tour(id).requires).toMatchObject({ modules: ['pages'], platforms: WDDL });
		expect(tour('graph.map').requires).toMatchObject({ modules: ['graph', 'pages'], platforms: WDDL });
		expect(tour('graph.enable').requires).toMatchObject({ role: 'owner', platforms: WDDL });
		expect(tour('routines.first').requires).toMatchObject({ modules: ['daily_routines'], platforms: ALL });
		expect(tour('routines.views').requires?.platforms).toEqual(ALL);
		expect(tour('routines.reminders').requires?.platforms).toEqual(WDDL);
	});

	it('triggers', () => {
		expect(tour('tasks.statuses').triggers).toContainEqual({ kind: 'feature_enabled', module: 'workflow' });
		expect(tour('tasks.categories').triggers).toContainEqual({ kind: 'first_visit', routeName: 'WorkspaceCategories' });
		expect(tour('tasks.files-page').triggers).toContainEqual({ kind: 'first_visit', routeName: 'WorkspaceFiles' });
		expect(tour('time.parallel').triggers).toContainEqual({ kind: 'event', event: 'timer.started', count: 2 });
		expect(tour('pages.first').triggers).toContainEqual({ kind: 'first_visit', routeName: 'WorkspacePages' });
		expect(tour('pages.links').triggers).toContainEqual({ kind: 'event', event: 'page.created', count: 2 });
		expect(tour('graph.map').triggers).toContainEqual({ kind: 'first_visit', routeName: 'WorkspaceMap' });
		expect(tour('graph.map').triggers).toContainEqual({ kind: 'feature_enabled', module: 'graph' });
		expect(tour('routines.first').triggers).toContainEqual({ kind: 'first_visit', routeName: 'DailyRoutines' });
	});

	it('wait steps listen for the planned events', () => {
		expect(waits('tasks.statuses')).toEqual(['status.created']);
		expect(waits('tasks.categories')).toEqual(['category.created']);
		expect(waits('tasks.relations')).toEqual(['relation.created']);
		expect(waits('time.checkpoints')).toEqual(['checkpoint.added']);
		expect(waits('time.pomodoro')).toEqual(['pomodoro.started']);
		expect(waits('time.export')).toEqual(['tasks.selected', 'tasks.actions_opened', 'export.downloaded']);
		expect(waits('pages.first')).toEqual(['page.created']);
		expect(waits('pages.context')).toEqual(['page.created', 'page.context_saved']);
		expect(tour('pages.context').steps[0].waitFor?.where).toEqual({ type: 'context' });
		expect(waits('pages.links')).toEqual(['page.link_added']);
		expect(waits('pages.task-from-selection')).toEqual(['task.created_from_page']);
		expect(waits('graph.enable')).toEqual(['module.enabled:graph']);
		expect(tour('graph.enable').completeOn).toBe('module.enabled:graph');
		expect(waits('routines.first')).toEqual(['routine.created', 'routine.completed']);
		expect(waits('routines.reminders')).toEqual(['routine.editor_opened']);
		expect(tour('pages.first').steps[0].alreadyDone).toBe('has_page');
		expect(tour('pages.context').steps[1].alreadyDone).toBe('has_context_page');
		expect(tour('routines.first').steps[0].alreadyDone).toBe('has_routine');
	});

	it('routes and navigation', () => {
		expect(tour('tasks.statuses').steps[0].route).toEqual({ name: 'WorkspaceBoard', params: 'currentWorkspace' });
		expect(tour('tasks.categories').steps[0].takeMeRoute).toEqual({ name: 'WorkspaceCategories', params: 'currentWorkspace' });
		expect(tour('tasks.categories').steps[1].route).toEqual({ name: 'WorkspaceCategories', params: 'currentWorkspace' });
		expect(tour('tasks.relations').steps[0].takeMeRoute).toEqual(LIST);
		expect(tour('tasks.files-page').steps[0].route).toEqual({ name: 'WorkspaceFiles', params: 'currentWorkspace' });
		expect(tour('tasks.multi-select').steps[0].route).toEqual(LIST);
		expect(tour('time.checkpoints').steps[0].takeMeRoute).toEqual(LIST);
		expect(tour('time.pomodoro').steps[0].takeMeRoute).toEqual(LIST);
		expect(tour('time.export').steps[0].route).toEqual(LIST);
		expect(tour('pages.first').steps[0].takeMeRoute).toEqual(PAGES);
		expect(tour('pages.context').steps[0].takeMeRoute).toEqual(PAGES);
		expect(tour('pages.links').steps[0].takeMeRoute).toEqual(PAGES);
		expect(tour('pages.versions').steps[0].takeMeRoute).toEqual(PAGES);
		expect(tour('pages.import').steps[0].route).toEqual(PAGES);
		expect(tour('pages.task-from-selection').steps[0].takeMeRoute).toEqual(PAGES);
		expect(tour('graph.map').steps[0].route).toEqual({ name: 'WorkspaceMap', params: 'currentWorkspace' });
		expect(tour('graph.enable').steps[0].route).toEqual({ name: 'ModuleSettings' });
		expect(tour('routines.views').steps[0].route).toEqual({ name: 'DailyRoutines' });
	});

	it('mobile fallbacks', () => {
		expect(tour('pages.first').steps[1].mobileAnchor).toBe('nav.menu-button');
		expect(tour('routines.first').steps[0].mobileAnchor).toBe('routines.quick-add');
		for (const id of S4B_IDS)
			for (const step of tour(id).steps)
				if (step.anchor === 'pages.tree' || step.anchor === 'pages.type-picker')
					expect(step.mobileAnchor || step.mobileBody).toBeTruthy();
	});

	it('optional steps', () => {
		const optional = (id: string) => tour(id).steps.map((s) => !!s.optional);
		expect(optional('tasks.statuses')).toEqual([false, true]);
		expect(optional('tasks.categories')).toEqual([false, true]);
		expect(optional('time.pomodoro')).toEqual([false, true]);
		expect(optional('routines.first')).toEqual([false, true]);
		expect(optional('routines.reminders')).toEqual([false, false, true]);
	});

	it('TI1 estimate step is anchored with the new copy', () => {
		const step = tour('time.timer').steps[3];
		expect(step.body).toBe('Set an estimate in task settings to see when you run over.');
	});

	it('does not ship dropped items', () => {
		expect(getTour('time.timer-prompt')).toBeUndefined();
		const text = JSON.stringify(S4B_IDS.map(tour));
		expect(text).not.toContain('map.node');
		expect(text).not.toContain('settings.timer-prompt');
		expect(text).not.toContain('pages.import_done');
		expect(tour('pages.import').completeOn).toBeUndefined();
	});
});
