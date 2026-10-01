import {
	assigneeWritePayload,
	hasMyPersonaAssignee,
	hasPersonaAssignee,
	personaAssigneeIds,
	visibleHumanAssignees,
} from '@/utils/personas';

const persona = (id: string, ownerId: number) => ({
	id,
	name: id,
	avatar_url: null,
	owner: { id: ownerId, name: `owner-${ownerId}` },
	workspace_id: null,
});

describe('visibleHumanAssignees', () => {
	it('hides humans that own an assigned persona', () => {
		const task = {
			assignees: [
				{ id: 1, name: 'Ann' },
				{ id: 2, name: 'Bob' },
			],
			persona_assignees: [persona('p1', 1)],
		};
		expect(visibleHumanAssignees(task)).toEqual([{ id: 2, name: 'Bob' }]);
	});

	it('keeps everyone when there are no personas or the field is missing', () => {
		const assignees = [{ id: 1 }, { id: 2 }];
		expect(visibleHumanAssignees({ assignees })).toEqual(assignees);
		expect(visibleHumanAssignees({ assignees, persona_assignees: [] })).toEqual(
			assignees,
		);
	});

	it('handles missing assignees', () => {
		expect(visibleHumanAssignees({})).toEqual([]);
		expect(visibleHumanAssignees({ assignees: null })).toEqual([]);
	});

	it('ignores uuid-only persona lists (already normalized for save)', () => {
		const assignees = [{ id: 1 }];
		expect(visibleHumanAssignees({ assignees, persona_assignees: ['p1'] })).toEqual(
			assignees,
		);
	});
});

describe('assigneeWritePayload', () => {
	it('keeps hidden owners in assignees and sends persona uuids alongside', () => {
		const task = {
			assignees: [{ id: 1 }, { id: 2 }],
			persona_assignees: [persona('p1', 1)],
		};
		expect(assigneeWritePayload(task)).toEqual({
			assignees: [1, 2],
			persona_assignees: ['p1'],
		});
	});

	it('uses the supplied human ids but still carries personas', () => {
		const task = { assignees: [{ id: 1 }], persona_assignees: [persona('p1', 1)] };
		expect(assigneeWritePayload(task, [1, 5])).toEqual({
			assignees: [1, 5],
			persona_assignees: ['p1'],
		});
	});

	it('sends an empty persona list when the task has none', () => {
		expect(assigneeWritePayload({ assignees: [{ id: 3 }] })).toEqual({
			assignees: [3],
			persona_assignees: [],
		});
	});
});

describe('persona membership helpers', () => {
	const task = { persona_assignees: [persona('p1', 7), persona('p2', 8)] };

	it('lists uuids, accepting objects or strings', () => {
		expect(personaAssigneeIds(task)).toEqual(['p1', 'p2']);
		expect(personaAssigneeIds({ persona_assignees: ['x'] })).toEqual(['x']);
		expect(personaAssigneeIds({})).toEqual([]);
	});

	it('checks a specific persona', () => {
		expect(hasPersonaAssignee(task, 'p2')).toBe(true);
		expect(hasPersonaAssignee(task, 'nope')).toBe(false);
	});

	it('detects a persona owned by me', () => {
		expect(hasMyPersonaAssignee(task, 8)).toBe(true);
		expect(hasMyPersonaAssignee(task, 1)).toBe(false);
		expect(hasMyPersonaAssignee(task, null)).toBe(false);
	});
});
