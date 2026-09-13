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

	it('falls back to the key when the backend grows a tool the UI has not learned', () => {
		expect(agentToolLabel('list_files')).toBe('list_files');
	});

	it('survives a missing tool name', () => {
		expect(agentToolLabel(undefined)).toBe('');
		expect(agentToolLabel('')).toBe('');
	});
});
