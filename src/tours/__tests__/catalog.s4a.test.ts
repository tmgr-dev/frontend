import { buildHubModel } from '../hubModel';
import { createTours } from '../engine';
import { createTourBus } from '../events';
import { createLease } from '../lease';
import { getTour } from '../catalog';
import type { Platform } from '../types';

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
		expect(tour('personas.assign').requires).toMatchObject({ modules: ['personas', 'task.assignees'], account: 'cloud', platforms: ['web', 'desktop'] });
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
		expect(tour('agents.work-log').steps[1].alreadyDone).toBeUndefined();
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
		expect(tour('ws.privacy').steps.map((s) => s.route)).toEqual([
			{ name: 'Settings', query: { tab: 'privacy' } },
			{ name: 'Settings', query: { tab: 'about' } },
		]);
		expect(tour('ws.telegram').steps[0].route).toEqual({ name: 'Settings', query: { tab: 'profile' } });
		expect(tour('ws.default-landing').steps[0].route).toEqual({ name: 'ModuleSettings' });
		for (const id of ['agents.work-log', 'personas.assign'])
			expect(tour(id).steps[0].takeMeRoute).toEqual({ name: 'WorkspaceTasksList', params: 'currentWorkspace' });
	});

	it('mobile fallbacks', () => {
		expect(tour('agents.ask-ai').steps[0].mobileBody).toBe('Tap Ask AI in the top bar.');
		expect(tour('ws.workspaces').steps[0].mobileAnchor).toBe('nav.menu-button');
		expect(tour('ws.notifications').steps[0].mobileBody).toBe('Tap the bell in the top bar.');
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

	describe('hub availability', () => {
		const memory = () => {
			const data = new Map<string, string>();
			return {
				getItem: (k: string) => data.get(k) ?? null,
				setItem: (k: string, v: string) => void data.set(k, v),
				removeItem: (k: string) => void data.delete(k),
			};
		};
		const hub = (platform: Platform, account: 'cloud' | 'local', modules: string[]) => {
			const engine = createTours({
				api: {
					fetchState: jest.fn().mockResolvedValue({}),
					putTour: jest.fn(),
					putPreferences: jest.fn(),
					resetTours: jest.fn(),
					setIntent: jest.fn(),
					fetchFacts: jest.fn(),
					removeStarterTasks: jest.fn(),
				} as never,
				bus: createTourBus(),
				lease: createLease({ storage: memory(), now: () => Date.now(), ownerId: 'a' }),
				storage: memory(),
				session: memory(),
				now: () => Date.now(),
				platform: () => platform,
				mobile: () => platform === 'mobile_web',
				account: () => account,
				isOwner: () => true,
				modulesKnown: () => true,
				isModuleEnabled: (key) => modules.includes(key),
				currentRouteName: () => null,
				navigate: () => {},
				isBusy: () => false,
				overlay: () => null,
				setTimer: () => () => {},
			});
			const items = engine.api.hubItems.value.filter((item) => S4A_IDS.includes(item.tour.id));
			const rows = buildHubModel(items, { canManageModules: true, isModuleEnabled: (key) => modules.includes(key) })
				.groups.flatMap((group) => group.rows);
			return { items, rows };
		};
		const everything = ['agent_work', 'alerts', 'mcp', 'ai.assistant', 'personas', 'task.assignees', 'dashboard', 'telegram'];
		const reason = (items: ReturnType<typeof hub>['items'], id: string) =>
			items.find((item) => item.tour.id === id)?.unavailableReason;

		it('shows every S4a tour on web with all modules on', () => {
			const { items } = hub('web', 'cloud', everything);
			expect(items.filter((item) => !item.available)).toEqual([]);
		});

		it('hides cloud-only tours for a local desktop account and keeps the local ones', () => {
			const { items, rows } = hub('desktop_local', 'local', everything);
			expect(reason(items, 'agents.work-log')).toBe('platform');
			expect(reason(items, 'ws.invite')).toBe('platform');
			expect(reason(items, 'personas.create')).toBe('platform');
			expect(reason(items, 'personas.assign')).toBe('platform');
			const available = items.filter((item) => item.available).map((item) => item.tour.id);
			expect(available).toEqual(expect.arrayContaining(['ws.modules', 'ws.workspaces', 'ws.appearance', 'ws.privacy']));
			expect(rows.map((row) => row.id)).not.toContain('agents.work-log');
		});

		it('hides platform-limited tours on mobile web', () => {
			const { items } = hub('mobile_web', 'cloud', everything);
			for (const id of ['agents.mcp-token', 'personas.create', 'personas.token', 'personas.assign', 'personas.skills', 'ws.telegram'])
				expect(reason(items, id)).toBe('platform');
			expect(reason(items, 'agents.ask-ai')).toBeUndefined();
		});

		it('hides the Ask AI tour on the desktop app', () => {
			expect(reason(hub('desktop', 'cloud', everything).items, 'agents.ask-ai')).toBe('platform');
		});

		it('greys module-off tours with the turn-on message', () => {
			const { rows } = hub('web', 'cloud', ['board']);
			const row = rows.find((candidate) => candidate.id === 'personas.create');
			expect(row).toMatchObject({ action: 'none' });
			expect(row?.disabledMessage).toContain('Turn on');
		});
	});
});
