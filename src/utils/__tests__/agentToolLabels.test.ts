import { agentToolLabel } from '../agentToolLabels';

describe('agentToolLabel', () => {
	it('names every tool the agent is allowed to call', () => {
		expect(agentToolLabel('get_task')).toBe('Reading the task');
		expect(agentToolLabel('list_tasks')).toBe('Listing tasks');
		expect(agentToolLabel('search_tasks')).toBe('Searching tasks');
		expect(agentToolLabel('list_comments')).toBe('Reading comments');
		expect(agentToolLabel('list_statuses')).toBe('Reading the board statuses');
		expect(agentToolLabel('list_members')).toBe('Reading the members');
		expect(agentToolLabel('list_routines')).toBe('Reading the routines');
		expect(agentToolLabel('time_summary')).toBe('Summing tracked time');
		expect(agentToolLabel('deadline_report')).toBe('Checking deadlines');
	});

	it('names the write tools that wait for confirmation', () => {
		const expected: Record<string, string> = {
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
		for (const [tool, label] of Object.entries(expected)) {
			expect(agentToolLabel(tool)).toBe(label);
		}
	});

	it('falls back to the key when the backend grows a tool the UI has not learned', () => {
		expect(agentToolLabel('list_files')).toBe('list_files');
	});

	it('survives a missing tool name', () => {
		expect(agentToolLabel(undefined)).toBe('');
		expect(agentToolLabel('')).toBe('');
	});
});
