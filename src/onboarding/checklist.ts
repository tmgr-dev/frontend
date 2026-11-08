import { t } from '@/i18n';
import { moduleOffMessage } from '@/tours/hubModel';
import type { Facts } from './poller';

export interface ChecklistItem {
	id: string;
	label: string;
	done: boolean;
	optional?: boolean;
	disabledMessage?: string;
}

export interface Checklist {
	items: ChecklistItem[];
	done: number;
	total: number;
}

export interface ChecklistDeps {
	isModuleEnabled(key: string): boolean;
	modulesKnown?(): boolean;
	canManageModules: boolean;
	account: 'cloud' | 'local' | 'none';
	emailVerified?: boolean;
}

interface Spec {
	id: string;
	label: string;
	fact: string;
	module?: string;
	optional?: boolean;
	cloudOnly?: boolean;
	needsVerifiedEmail?: boolean;
}

const HUMAN: Spec[] = [
	{
		id: 'create-task',
		label: 'onboarding.helpers.checklist.human.createTask',
		fact: 'has_task',
	},
	{
		id: 'run-timer',
		label: 'onboarding.helpers.checklist.human.runTimer',
		fact: 'has_timer_minute',
		module: 'task.countdown',
	},
	{
		id: 'move-card',
		label: 'onboarding.helpers.checklist.human.moveCard',
		fact: 'has_board_move',
		module: 'board',
	},
	{
		id: 'add-routine',
		label: 'onboarding.helpers.checklist.human.addRoutine',
		fact: 'has_routine',
		module: 'daily_routines',
	},
	{
		id: 'write-page',
		label: 'onboarding.helpers.checklist.human.writePage',
		fact: 'has_page',
		module: 'pages',
	},
	{
		id: 'connect-agent',
		label: 'onboarding.helpers.checklist.human.connectAgent',
		fact: 'mcp_first_call_at',
		module: 'mcp',
		optional: true,
		cloudOnly: true,
	},
];

const AGENT: Spec[] = [
	{
		id: 'connect-agent',
		label: 'onboarding.helpers.checklist.agent.connectAgent',
		fact: 'mcp_first_call_at',
		module: 'mcp',
		cloudOnly: true,
	},
	{
		id: 'first-agent-run',
		label: 'onboarding.helpers.checklist.agent.firstAgentRun',
		fact: 'agent_runs_finished',
		module: 'agent_work',
		cloudOnly: true,
	},
	{
		id: 'phone-alert',
		label: 'onboarding.helpers.checklist.agent.phoneAlert',
		fact: 'notify_token',
		module: 'alerts',
		cloudOnly: true,
	},
	{
		id: 'start-timer',
		label: 'onboarding.helpers.checklist.agent.startTimer',
		fact: 'has_timer_minute',
		module: 'task.countdown',
	},
	{
		id: 'context-page',
		label: 'onboarding.helpers.checklist.agent.contextPage',
		fact: 'has_context_page',
		module: 'pages',
	},
	{
		id: 'invite-teammate',
		label: 'onboarding.helpers.checklist.agent.inviteTeammate',
		fact: 'invites_sent',
		cloudOnly: true,
		needsVerifiedEmail: true,
	},
];

export function buildChecklist(
	intent: string | null,
	facts: Facts,
	deps: ChecklistDeps,
): Checklist {
	const specs = (intent === 'agent' ? AGENT : HUMAN).filter(
		(spec) => !spec.cloudOnly || deps.account === 'cloud',
	);
	const known = deps.modulesKnown ? deps.modulesKnown() : true;
	const emailVerified = deps.emailVerified ?? !!facts.email_verified;
	const items = specs.map((spec): ChecklistItem => {
		const item: ChecklistItem = {
			id: spec.id,
			label: t(spec.label),
			done: !!facts[spec.fact],
		};
		if (spec.optional) item.optional = true;
		if (spec.module && known && !deps.isModuleEnabled(spec.module)) {
			item.disabledMessage = moduleOffMessage(
				spec.module,
				deps.canManageModules,
			);
		} else if (spec.needsVerifiedEmail && !emailVerified) {
			item.disabledMessage = t('onboarding.helpers.checklist.verifyEmail');
		}
		return item;
	});
	return {
		items,
		done: items.filter((item) => item.done).length,
		total: items.length,
	};
}
