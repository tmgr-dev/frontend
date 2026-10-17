import type { TourDef } from '../types';
import { startTours } from './start';

const LIST = {
	name: 'WorkspaceTasksList',
	params: 'currentWorkspace',
} as const;
const CLOUD_APP = ['web', 'mobile_web', 'desktop'] as const;
const connectAgent = startTours.find(
	(tour) => tour.id === 'start.connect-agent',
)!;

export const agentsTours: TourDef[] = [
	{
		id: 'agents.work-log',
		version: 1,
		since: '3.4.156',
		group: 'agents',
		title: 'Watch an agent log its work',
		minutes: 2,
		triggers: [{ kind: 'hub' }],
		requires: {
			modules: ['agent_work'],
			account: 'cloud',
			platforms: [...CLOUD_APP],
		},
		steps: [
			{
				id: 's1',
				type: 'wait',
				anchor: 'task-form.agent-work',
				waitFor: { event: 'agent.run_started' },
				takeMeRoute: LIST,
				title: 'Agent runs',
				body: "Your agent's runs appear here: branch, commits, tests and time.",
			},
			{
				id: 's2',
				type: 'wait',
				anchor: 'task-form.agent-work',
				waitFor: { event: 'agent.run_finished' },
				title: 'Run in progress',
				body: 'Waiting for the agent to finish its run…',
			},
			{
				id: 's3',
				type: 'spot',
				anchor: 'task-form.agent-totals',
				optional: true,
				title: 'Agent time',
				body: 'Agent time never adds to your tracked time. You see both.',
			},
		],
	},
	{
		id: 'agents.phone-alerts',
		version: 1,
		since: '3.4.156',
		group: 'agents',
		title: 'Phone alerts for agents',
		minutes: 2,
		triggers: [{ kind: 'hub' }],
		requires: {
			modules: ['alerts'],
			account: 'cloud',
			platforms: [...CLOUD_APP],
		},
		steps: [
			{
				id: 's1',
				type: 'wait',
				anchor: 'agent-notify.connect',
				waitFor: { event: 'notify.token_created' },
				alreadyDone: 'notify_token',
				route: { name: 'AgentNotificationSettings' },
				title: 'Create a notify token',
				body: 'Create a notify token so your agent can ping your phone.',
			},
			{
				id: 's2',
				type: 'tip',
				anchor: 'agent-notify.command',
				optional: true,
				title: 'Add it to your agent',
				body: 'Add npx -y @tmgr/notify mcp to your agent, then send a test.',
			},
			{
				id: 's3',
				type: 'tip',
				anchor: 'agent-notify.test',
				optional: true,
				title: 'Send a test',
				body: 'iPhone (TestFlight beta) rings through Silent mode on iOS 26 or later.',
			},
		],
	},
	{
		id: 'agents.mcp-token',
		version: 1,
		since: '3.4.156',
		group: 'agents',
		title: 'Personal MCP token',
		minutes: 1,
		triggers: [{ kind: 'hub' }],
		requires: {
			modules: ['mcp'],
			account: 'cloud',
			platforms: ['web', 'desktop'],
		},
		steps: [
			{
				id: 's1',
				type: 'wait',
				anchor: 'settings.device-token',
				waitFor: { event: 'mcp.token_created' },
				route: { name: 'Settings', query: { tab: 'device' } },
				title: 'Create a token',
				body: 'This token lets any MCP client act as you. Shown once, stored hashed.',
			},
		],
	},
	{
		id: 'agents.ask-ai',
		version: 1,
		since: '3.4.156',
		group: 'agents',
		title: 'Ask AI about your workspace',
		minutes: 1,
		triggers: [{ kind: 'hub' }],
		requires: {
			modules: ['ai.assistant'],
			account: 'cloud',
			platforms: ['web', 'mobile_web'],
		},
		completeOn: 'ai.question_sent',
		steps: [
			{
				id: 's1',
				type: 'wait',
				anchor: 'nav.ask-ai',
				waitFor: { event: 'ai.panel_opened' },
				title: 'Open Ask AI',
				body: 'Ask questions about tasks, time and deadlines.',
				mobileBody: 'Tap Ask AI in the top bar.',
			},
			{
				id: 's2',
				type: 'wait',
				anchor: 'ai.input',
				waitFor: { event: 'ai.question_sent' },
				title: 'Ask a question',
				body: 'Try: what did I work on this week?',
			},
		],
	},
	{
		id: 'agents.connect-later',
		version: 1,
		since: '3.4.156',
		group: 'agents',
		title: 'Connect an agent',
		minutes: connectAgent.minutes,
		triggers: [{ kind: 'hub' }],
		requires: connectAgent.requires,
		steps: connectAgent.steps,
	},
];
