import { createTaskVersionGuard } from '../taskVersionGuard';

const task = (updated_at: string) => ({ id: 1, updated_at });

describe('createTaskVersionGuard', () => {
	it('ignores the echo of a version already known from an own response', async () => {
		const notify = jest.fn();
		const guard = createTaskVersionGuard(notify);
		await guard.track(async () => task('2026-10-04T10:00:05Z'));
		guard.receive(task('2026-10-04T10:00:05Z'));
		expect(notify).not.toHaveBeenCalled();
	});

	it('ignores an older event', async () => {
		const notify = jest.fn();
		const guard = createTaskVersionGuard(notify);
		guard.recordKnown(task('2026-10-04T10:00:05Z'));
		guard.receive(task('2026-10-04T10:00:01Z'));
		expect(notify).not.toHaveBeenCalled();
	});

	it('notifies for a newer version after an own write', async () => {
		const notify = jest.fn();
		const guard = createTaskVersionGuard(notify);
		await guard.track(async () => task('2026-10-04T10:00:05Z'));
		guard.receive(task('2026-10-04T10:00:09Z'));
		expect(notify).toHaveBeenCalledTimes(1);
	});

	it('does not notify for an echo that arrives before the own response', async () => {
		const notify = jest.fn();
		const guard = createTaskVersionGuard(notify);
		let resolve!: (t: any) => void;
		const pending = guard.track(() => new Promise((r) => (resolve = r)));
		guard.receive(task('2026-10-04T10:00:05Z'));
		expect(notify).not.toHaveBeenCalled();
		resolve(task('2026-10-04T10:00:05Z'));
		await pending;
		expect(notify).not.toHaveBeenCalled();
	});

	it('notifies for a newer event that arrived while the own write was pending', async () => {
		const notify = jest.fn();
		const guard = createTaskVersionGuard(notify);
		let resolve!: (t: any) => void;
		const pending = guard.track(() => new Promise((r) => (resolve = r)));
		guard.receive(task('2026-10-04T10:00:09Z'));
		resolve(task('2026-10-04T10:00:05Z'));
		await pending;
		expect(notify).toHaveBeenCalledTimes(1);
	});

	it('notifies when the event carries no version', () => {
		const notify = jest.fn();
		createTaskVersionGuard(notify).receive({ id: 1 });
		expect(notify).toHaveBeenCalledTimes(1);
	});
});
