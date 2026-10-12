import { getTour } from '../catalog';

const ALL = ['web', 'mobile_web', 'desktop', 'desktop_local'];
const tour = (id: string) => getTour(id)!;
const anchors = (id: string) => tour(id).steps.map((s) => s.anchor);

const S4A_IDS = [
	'agents.work-log',
	'agents.phone-alerts',
	'agents.mcp-token',
	'agents.ask-ai',
	'agents.connect-later',
	'personas.create',
	'personas.token',
	'personas.assign',
	'personas.skills',
	'ws.modules',
	'ws.invite',
	'ws.dashboard',
	'ws.workspaces',
	'ws.appearance',
	'ws.notifications',
	'ws.privacy',
	'ws.telegram',
	'ws.default-landing',
];

describe('S4a catalog', () => {
	it('registers every S4a tour with the right group', () => {
		for (const id of S4A_IDS) {
			expect(tour(id)).toBeDefined();
			expect(tour(id).group).toBe(id.split('.')[0] === 'ws' ? 'workspace' : id.split('.')[0]);
			expect(tour(id).version).toBe(1);
			expect(tour(id).triggers).toEqual(expect.arrayContaining([{ kind: 'hub' }]));
		}
	});

	it('names every anchor of the S4a table', () => {
		expect(anchors('agents.work-log')).toEqual(['task-form.agent-work', 'task-form.agent-work', 'task-form.agent-totals']);
		expect(anchors('agents.phone-alerts')).toEqual(['agent-notify.connect', 'agent-notify.command', 'agent-notify.test']);
		expect(anchors('agents.mcp-token')).toEqual(['settings.device-token']);
		expect(anchors('agents.ask-ai')).toEqual(['nav.ask-ai', 'ai.input']);
		expect(anchors('personas.create')).toEqual(['personas.create', 'personas.grants']);
		expect(anchors('personas.token')).toEqual(['personas.tokens']);
		expect(anchors('personas.assign')).toEqual(['task-form.assignees', 'task-form.persona-assignees']);
		expect(anchors('personas.skills')).toEqual(['personas.skills']);
		expect(anchors('ws.modules')).toEqual(['modules.presets', 'modules.list', 'modules.hide']);
		expect(anchors('ws.invite')).toEqual(['workspace.members']);
		expect(anchors('ws.dashboard')).toEqual(['dashboard.activity', 'dashboard.member']);
		expect(anchors('ws.workspaces')).toEqual(['nav.workspace-switcher']);
		expect(anchors('ws.appearance')).toEqual(['settings.theme']);
		expect(anchors('ws.notifications')).toEqual(['nav.notification-bell']);
		expect(anchors('ws.privacy')).toEqual(['settings.privacy-telemetry', 'settings.tips']);
		expect(anchors('ws.telegram')).toEqual(['settings.telegram']);
		expect(anchors('ws.default-landing')).toEqual(['settings.default-landing']);
	});

	it('wait steps that navigate carry no route', () => {
		for (const id of S4A_IDS)
			for (const step of tour(id).steps)
				if (step.type === 'wait' && step.takeMeRoute) expect(step.route).toBeUndefined();
	});

	it('tours with a tip after the last wait step do not complete on the wait event', () => {
		for (const id of S4A_IDS) {
			const { steps, completeOn } = tour(id);
			const lastWait = steps.map((s) => s.type).lastIndexOf('wait');
			if (lastWait >= 0 && lastWait < steps.length - 1) expect(`${id}: ${completeOn}`).toBe(`${id}: undefined`);
		}
		expect(tour('agents.work-log').completeOn).toBeUndefined();
		expect(tour('agents.ask-ai').completeOn).toBe('ai.question_sent');
	});

	it('cloud-only tours exclude desktop_local', () => {
		for (const id of S4A_IDS) {
			const { requires } = tour(id);
			if (requires?.account === 'cloud') expect(requires.platforms).not.toContain('desktop_local');
		}
	});

	it('platforms and requirements follow the plan', () => {
		expect(tour('agents.work-log').requires).toMatchObject({ modules: ['agent_work'], account: 'cloud', platforms: ['web', 'mobile_web', 'desktop'] });
		expect(tour('agents.phone-alerts').requires).toMatchObject({ modules: ['alerts'], account: 'cloud', platforms: ['web', 'mobile_web', 'desktop'] });
		expect(tour('agents.mcp-token').requires).toMatchObject({ modules: ['mcp'], account: 'cloud', platforms: ['web', 'desktop'] });
		expect(tour('agents.ask-ai').requires).toMatchObject({ modules: ['ai.assistant'], account: 'cloud', platforms: ['web', 'mobile_web'] });
		expect(tour('agents.ask-ai').requires?.platforms).not.toContain('desktop');
		expect(tour('agents.ask-ai').requires?.platforms).not.toContain('desktop_local');
		expect(tour('personas.create').requires).toMatchObject({ modules: ['personas'], account: 'cloud', platforms: ['web', 'desktop'] });
		expect(tour('personas.assign').requires).toMatchObject({ modules: ['personas', 'task.assignees'], platforms: ['web', 'desktop', 'desktop_local'] });
		expect(tour('personas.assign').requires?.account).toBeUndefined();
		expect(tour('ws.modules').requires?.platforms).toEqual(ALL);
		expect(tour('ws.invite').requires).toMatchObject({ account: 'cloud', platforms: ['web', 'mobile_web', 'desktop'] });
		expect(tour('ws.dashboard').requires).toMatchObject({ modules: ['dashboard'], account: 'cloud' });
		expect(tour('ws.telegram').requires).toMatchObject({ modules: ['telegram'], account: 'cloud', platforms: ['web', 'desktop'] });
		expect(tour('ws.default-landing').requires?.platforms).toEqual(['web', 'mobile_web', 'desktop']);
	});

	it('first_visit triggers', () => {
		expect(tour('personas.create').triggers).toContainEqual({ kind: 'first_visit', routeName: 'PersonaSettings' });
		expect(tour('ws.modules').triggers).toContainEqual({ kind: 'first_visit', routeName: 'ModuleSettings' });
		expect(tour('ws.dashboard').triggers).toContainEqual({ kind: 'first_visit', routeName: 'WorkspaceDashboard' });
	});

	it('wait steps listen for the planned events', () => {
		const waits = (id: string) => tour(id).steps.filter((s) => s.type === 'wait').map((s) => s.waitFor?.event);
		expect(waits('agents.work-log')).toEqual(['agent.run_started', 'agent.run_finished']);
		expect(tour('agents.work-log').steps[1].alreadyDone).toBe('agent_runs_finished');
		expect(waits('agents.phone-alerts')).toEqual(['notify.token_created']);
		expect(tour('agents.phone-alerts').steps[0].alreadyDone).toBe('notify_token');
		expect(waits('agents.mcp-token')).toEqual(['mcp.token_created']);
		expect(waits('agents.ask-ai')).toEqual(['ai.panel_opened', 'ai.question_sent']);
		expect(waits('personas.create')).toEqual(['persona.created']);
		expect(waits('personas.token')).toEqual(['persona.token_created']);
		expect(waits('personas.assign')).toEqual(['persona.assigned']);
		expect(waits('ws.invite')).toEqual(['invite.sent']);
		expect(tour('ws.invite').steps[0].alreadyDone).toBe('invites_sent');
		expect(waits('ws.telegram')).toEqual([]);
	});

	it('routes and queries', () => {
		expect(tour('agents.phone-alerts').steps[0].route).toEqual({ name: 'AgentNotificationSettings' });
		expect(tour('agents.mcp-token').steps[0].route).toEqual({ name: 'Settings', query: { tab: 'device' } });
		expect(tour('personas.create').steps[0].route).toEqual({ name: 'PersonaSettings' });
		expect(tour('personas.token').steps[0].route).toEqual({ name: 'PersonaSettings' });
		expect(tour('personas.skills').steps[0].route).toEqual({ name: 'PersonaSettings' });
		expect(tour('ws.invite').steps[0].route).toEqual({ name: 'WorkspaceSettings' });
		expect(tour('ws.appearance').steps[0].route).toEqual({ name: 'Settings', query: { tab: 'theme' } });
		expect(tour('ws.privacy').steps.map((s) => s.route)).toEqual(Array(2).fill({ name: 'Settings', query: { tab: 'about' } }));
		expect(tour('ws.telegram').steps[0].route).toEqual({ name: 'Settings', query: { tab: 'profile' } });
		expect(tour('ws.default-landing').steps[0].route).toEqual({ name: 'ModuleSettings' });
		for (const id of ['agents.work-log', 'personas.assign'])
			expect(tour(id).steps[0].takeMeRoute).toEqual({ name: 'WorkspaceTasksList', params: 'currentWorkspace' });
	});

	it('mobile fallbacks', () => {
		expect(tour('agents.ask-ai').steps[0].mobileBody).toBeTruthy();
		expect(tour('ws.workspaces').steps[0].mobileAnchor).toBe('nav.menu-button');
		expect(tour('ws.notifications').steps[0].mobileBody).toBeTruthy();
	});

	it('optional steps', () => {
		const optional = (id: string) => tour(id).steps.map((s) => !!s.optional);
		expect(optional('agents.work-log')).toEqual([false, false, true]);
		expect(optional('agents.phone-alerts')).toEqual([false, true, true]);
		expect(optional('personas.assign')).toEqual([false, true]);
		expect(optional('ws.modules')).toEqual([true, false, true]);
		expect(optional('ws.dashboard')).toEqual([false, true]);
	});

	it('connect-later copies the connect-agent steps', () => {
		const later = tour('agents.connect-later');
		const g3 = tour('start.connect-agent');
		expect(later.steps).toEqual(g3.steps);
		expect(later.requires).toEqual(g3.requires);
	});

	it('does not ship dropped items', () => {
		expect(getTour('time.pomodoro-prompt')).toBeUndefined();
		expect(tour('ws.telegram').completeOn).toBeUndefined();
		const text = JSON.stringify(S4A_IDS.map(tour));
		expect(text).not.toContain('telegram.linked');
		expect(text).not.toContain('map.node');
		expect(text).not.toContain('settings.privacy-tips');
	});

	it('ships every S4a tour since 3.4.155', () => {
		for (const id of S4A_IDS) {
			expect(tour(id).since).toBe('3.4.155');
		}
	});
});
