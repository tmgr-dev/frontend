import { t } from '@/i18n';

/**
 * Names for the workspace agent's tools (TM-223). The panel used to print the raw key
 * (`list_tasks`), which reads like an internal detail leaking into the product. Keys the UI has
 * not learned yet fall back to the key itself rather than to a vague "working…", so a tool added
 * on the backend is still legible here.
 */
const KNOWN_TOOLS = new Set([
	'get_task',
	'list_tasks',
	'search_tasks',
	'list_comments',
	'list_statuses',
	'list_members',
	'list_routines',
	'time_summary',
	'deadline_report',
	'routine_agenda',
	'get_routine',
	'create_task',
	'add_comment',
	'update_task',
	'add_checkpoint',
	'start_timer',
	'stop_timer',
	'link_tasks',
	'create_routine',
	'update_routine',
	'set_routine_state',
	'move_routine_instance',
	'convert_routine',
]);

export function agentToolLabel(tool: string | undefined | null): string {
	if (!tool) return '';
	return KNOWN_TOOLS.has(tool) ? t(`agents.helpers.toolLabels.${tool}`) : tool;
}
