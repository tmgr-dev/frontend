import { taskCreator } from '@/utils/taskCreator';

const members = [
	{ id: 1, name: 'Ada Lovelace', has_avatar: true },
	{ id: 2, name: 'Grace Hopper' },
];

describe('taskCreator', () => {
	it('finds the member who created the task', () => {
		expect(taskCreator(members, 2, false)).toEqual(members[1]);
	});

	it('accepts a numeric string id', () => {
		expect(taskCreator(members, '1', false)).toEqual(members[0]);
	});

	it('is null when the creator is no longer a member', () => {
		expect(taskCreator(members, 99, false)).toBeNull();
	});

	it('is null while members are not loaded', () => {
		expect(taskCreator([], 1, false)).toBeNull();
		expect(taskCreator(null, 1, false)).toBeNull();
		expect(taskCreator(undefined, 1, false)).toBeNull();
	});

	it('is null without a creator id', () => {
		expect(taskCreator(members, 0, false)).toBeNull();
		expect(taskCreator(members, null, false)).toBeNull();
		expect(taskCreator(members, undefined, false)).toBeNull();
	});

	it('is null for a local workspace task', () => {
		expect(taskCreator(members, 1, true)).toBeNull();
	});
});
