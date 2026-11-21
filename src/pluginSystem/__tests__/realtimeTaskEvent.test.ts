import { realtimeTaskEvent } from '../realtimeTaskEvent';

const OWN = '11111111-1111-4111-8111-111111111111';
const OTHER = '22222222-2222-4222-8222-222222222222';

it('drops the echo of a write made on this instance', () => {
	expect(realtimeTaskEvent(5, { id: 1 }, 'updated', OWN, OWN)).toBeNull();
});

it('drops the echo of a write made by a form on this instance', () => {
	expect(
		realtimeTaskEvent(5, { id: 1 }, 'updated', `${OWN}.form-abc`, OWN),
	).toBeNull();
	expect(
		realtimeTaskEvent(5, { id: 1 }, 'updated', `${OTHER}.form-abc`, OWN),
	).not.toBeNull();
});

it('delivers writes from other instances and from clients that sent no id', () => {
	expect(
		realtimeTaskEvent(5, { id: 1, title: 'a' }, 'updated', OTHER, OWN),
	).toEqual({
		type: 'task.updated',
		workspaceId: 5,
		taskId: 1,
		task: { id: 1, title: 'a' },
	});
	expect(
		realtimeTaskEvent(5, { id: 2 }, 'created', undefined, OWN),
	).toMatchObject({ type: 'task.created', taskId: 2 });
});

it('maps a deletion without the task and sets no actor or changed list', () => {
	const event = realtimeTaskEvent(5, { id: 3 }, 'deleted', OTHER, OWN);
	expect(event).toEqual({ type: 'task.deleted', workspaceId: 5, taskId: 3 });
	expect(
		realtimeTaskEvent(5, { id: 3 }, 'updated', OTHER, OWN),
	).not.toHaveProperty('actor');
});

it('ignores a payload without a task id', () => {
	expect(realtimeTaskEvent(5, undefined, 'updated', OTHER, OWN)).toBeNull();
	expect(realtimeTaskEvent(5, {}, 'created', OTHER, OWN)).toBeNull();
});
