import type { CursorAgent } from '../../types/cursor';
import {
	ACTIVE_CURSOR_AGENT_STATUSES,
	isActiveCursorAgent,
	filterActiveCursorAgents,
	cursorAgentLabel,
	cursorAgentTaskRoute,
} from '../cursorAgents';

const makeAgent = (overrides: Partial<CursorAgent> = {}): CursorAgent => ({
	id: 1,
	cursor_agent_id: 'bc_abc123',
	task_id: 42,
	user_id: 7,
	project_category_id: 3,
	status: 'RUNNING',
	branch_name: 'feature/TM-156',
	pr_url: null,
	summary: null,
	error_message: null,
	started_at: '2026-06-23T10:00:00Z',
	finished_at: null,
	created_at: '2026-06-23T09:59:00Z',
	updated_at: '2026-06-23T10:00:00Z',
	...overrides,
});

describe('cursorAgents helpers', () => {
	describe('isActiveCursorAgent', () => {
		it('treats RUNNING and CREATING as active', () => {
			expect(isActiveCursorAgent(makeAgent({ status: 'RUNNING' }))).toBe(true);
			expect(isActiveCursorAgent(makeAgent({ status: 'CREATING' }))).toBe(true);
		});

		it('treats FINISHED, STOPPED and FAILED as inactive', () => {
			expect(isActiveCursorAgent(makeAgent({ status: 'FINISHED' }))).toBe(
				false,
			);
			expect(isActiveCursorAgent(makeAgent({ status: 'STOPPED' }))).toBe(false);
			expect(isActiveCursorAgent(makeAgent({ status: 'FAILED' }))).toBe(false);
		});

		it('keeps ACTIVE_CURSOR_AGENT_STATUSES in sync', () => {
			expect([...ACTIVE_CURSOR_AGENT_STATUSES].sort()).toEqual(
				['CREATING', 'RUNNING'].sort(),
			);
		});
	});

	describe('filterActiveCursorAgents', () => {
		it('returns only active agents, preserving order', () => {
			const agents = [
				makeAgent({ id: 1, status: 'RUNNING' }),
				makeAgent({ id: 2, status: 'FINISHED' }),
				makeAgent({ id: 3, status: 'CREATING' }),
				makeAgent({ id: 4, status: 'FAILED' }),
			];

			expect(filterActiveCursorAgents(agents).map((a) => a.id)).toEqual([1, 3]);
		});

		it('returns an empty array when given an empty or non-array input', () => {
			expect(filterActiveCursorAgents([])).toEqual([]);
			expect(
				filterActiveCursorAgents(undefined as unknown as CursorAgent[]),
			).toEqual([]);
			expect(
				filterActiveCursorAgents(null as unknown as CursorAgent[]),
			).toEqual([]);
		});
	});

	describe('cursorAgentLabel', () => {
		it('prefers branch_name, then summary, then cursor_agent_id', () => {
			expect(cursorAgentLabel(makeAgent({ branch_name: 'feature/x' }))).toBe(
				'feature/x',
			);
			expect(
				cursorAgentLabel(
					makeAgent({ branch_name: null, summary: 'Fixing bug' }),
				),
			).toBe('Fixing bug');
			expect(
				cursorAgentLabel(
					makeAgent({
						branch_name: null,
						summary: null,
						cursor_agent_id: 'bc_z',
					}),
				),
			).toBe('bc_z');
		});

		it('falls back to a task label when nothing else is present', () => {
			expect(
				cursorAgentLabel(
					makeAgent({
						branch_name: null,
						summary: null,
						cursor_agent_id: '',
						task_id: 99,
					}),
				),
			).toBe('Task #99');
		});
	});

	describe('cursorAgentTaskRoute', () => {
		it('builds a WorkspaceTask route from the agent task_id and workspace code', () => {
			expect(
				cursorAgentTaskRoute(makeAgent({ task_id: 42 }), 'tmgrdev'),
			).toEqual({
				name: 'WorkspaceTask',
				params: { workspace_code: 'tmgrdev', task_id: 42 },
			});
		});
	});
});
