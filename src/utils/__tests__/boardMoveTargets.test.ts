import { moveTargetStatuses } from '../boardMoveTargets';

const statuses = [
	{ id: 1, name: 'Backlog', type: 'default' },
	{ id: 2, name: 'In progress', type: 'active' },
	{ id: 3, name: 'Review', type: 'active' },
	{ id: 4, name: 'Done', type: 'completed' },
	{ id: 5, name: 'Archived', type: 'archived' },
];

describe('moveTargetStatuses', () => {
	it('offers every board status except the current one, in board order', () => {
		expect(moveTargetStatuses(statuses, 2).map((s) => s.id)).toEqual([1, 3, 4]);
	});

	it('leaves the archive out, it has its own confirmed action', () => {
		expect(moveTargetStatuses(statuses, 1).map((s) => s.type)).not.toContain(
			'archived',
		);
	});

	it('matches the current status whether the id comes as a number or a string', () => {
		expect(moveTargetStatuses(statuses, '3').map((s) => s.id)).toEqual([
			1, 2, 4,
		]);
	});

	it('follows the current column order after columns are dragged', () => {
		const reordered = [
			{ id: 1, type: 'default', pivot: { order: 3 } },
			{ id: 2, type: 'active', pivot: { order: 1 } },
			{ id: 3, type: 'active', pivot: { order: 2 } },
		];
		expect(moveTargetStatuses(reordered, 99).map((s) => s.id)).toEqual([
			2, 3, 1,
		]);
	});

	it('keeps the given order when the board has no order yet', () => {
		const unordered = [
			{ id: 1, type: 'default', pivot: { order: null } },
			{ id: 2, type: 'active', pivot: null },
			{ id: 3, type: 'active' },
		];
		expect(moveTargetStatuses(unordered, 99).map((s) => s.id)).toEqual([
			1, 2, 3,
		]);
	});

	it('returns an empty list before statuses load', () => {
		expect(moveTargetStatuses(null, 1)).toEqual([]);
		expect(moveTargetStatuses(undefined, undefined)).toEqual([]);
	});
});
