/**
 * Names for the workspace agent's tools (TM-223). The panel used to print the raw key
 * (`list_tasks`), which reads like an internal detail leaking into the product. Keys the UI has
 * not learned yet fall back to the key itself rather than to a vague "working…", so a tool added
 * on the backend is still legible here.
 */
const LABELS: Record<string, string> = {
	get_task: 'Reading the task',
	list_tasks: 'Listing tasks',
	search_tasks: 'Searching tasks',
	list_comments: 'Reading comments',
	list_statuses: 'Reading the board statuses',
	list_members: 'Reading the members',
	list_routines: 'Reading the routines',
	time_summary: 'Summing tracked time',
	deadline_report: 'Checking deadlines',
	routine_agenda: 'Reading your routines',
	get_routine: 'Reading the routine',
	create_task: 'Preparing a task',
	add_comment: 'Preparing a comment',
	update_task: 'Preparing a task update',
	add_checkpoint: 'Preparing a checkpoint',
	start_timer: 'Preparing to start the timer',
	stop_timer: 'Preparing to stop the timer',
	link_tasks: 'Preparing a task link',
	create_routine: 'Preparing a routine',
	update_routine: 'Preparing a routine change',
	set_routine_state: 'Preparing a routine check-off',
	move_routine_instance: 'Preparing to move a routine',
	convert_routine: 'Preparing to turn a routine into a task',
};

export function agentToolLabel(tool: string | undefined | null): string {
	if (!tool) return '';
	return LABELS[tool] ?? tool;
}
