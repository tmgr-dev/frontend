import type { CursorAgent } from '../../types/cursor';
import { createCursorConversationSync } from '../cursorConversationSync';

const agent = (id: number, status: CursorAgent['status']) =>
	({ id, status }) as CursorAgent;

describe('createCursorConversationSync', () => {
	let fetchConversation: jest.Mock;
	let errorSpy: jest.SpyInstance;

	beforeEach(() => {
		fetchConversation = jest.fn().mockResolvedValue({ messages: [] });
		errorSpy = jest.spyOn(console, 'error').mockImplementation(() => {});
	});

	afterEach(() => errorSpy.mockRestore());

	it('syncs every non-creating agent once when first seen', async () => {
		const sync = createCursorConversationSync(5, fetchConversation);

		const changed = await sync.update([
			agent(1, 'FINISHED'),
			agent(2, 'RUNNING'),
			agent(3, 'CREATING'),
		]);

		expect(changed).toBe(true);
		expect(fetchConversation.mock.calls).toEqual([
			[5, 1],
			[5, 2],
		]);
	});

	it('syncs again when an active agent finishes, then not for the same state', async () => {
		const sync = createCursorConversationSync(5, fetchConversation);
		await sync.update([agent(2, 'RUNNING')]);
		fetchConversation.mockClear();

		expect(await sync.update([agent(2, 'RUNNING')])).toBe(false);
		expect(fetchConversation).not.toHaveBeenCalled();

		expect(await sync.update([agent(2, 'FINISHED')])).toBe(true);
		expect(fetchConversation).toHaveBeenCalledTimes(1);
		expect(fetchConversation).toHaveBeenCalledWith(5, 2);

		expect(await sync.update([agent(2, 'FINISHED')])).toBe(false);
		expect(fetchConversation).toHaveBeenCalledTimes(1);
	});

	it('syncs a launched agent only after it leaves CREATING', async () => {
		const sync = createCursorConversationSync(5, fetchConversation);
		await sync.update([]);
		await sync.update([agent(9, 'CREATING')]);
		expect(fetchConversation).not.toHaveBeenCalled();

		await sync.update([agent(9, 'RUNNING')]);
		expect(fetchConversation).not.toHaveBeenCalled();

		await sync.update([agent(9, 'FAILED')]);
		expect(fetchConversation).toHaveBeenCalledTimes(1);
	});

	it('shares one request between overlapping updates for the same agent', async () => {
		let resolve: (v: unknown) => void = () => {};
		fetchConversation.mockReturnValue(new Promise((r) => (resolve = r)));
		const sync = createCursorConversationSync(5, fetchConversation);

		const first = sync.update([agent(1, 'RUNNING')]);
		const second = sync.update([agent(1, 'FINISHED')]);
		resolve({});
		await Promise.all([first, second]);

		expect(fetchConversation).toHaveBeenCalledTimes(1);
	});

	it('swallows errors, logs them and reports no change', async () => {
		fetchConversation.mockRejectedValue(new Error('boom'));
		const sync = createCursorConversationSync(5, fetchConversation);

		await expect(sync.update([agent(1, 'FINISHED')])).resolves.toBe(false);
		expect(errorSpy).toHaveBeenCalled();
	});

	it('retries a failed sync of a finished agent on the next update', async () => {
		fetchConversation.mockRejectedValueOnce(new Error('boom'));
		const sync = createCursorConversationSync(5, fetchConversation);

		await sync.update([agent(1, 'FINISHED')]);
		expect(await sync.update([agent(1, 'FINISHED')])).toBe(true);
		expect(fetchConversation).toHaveBeenCalledTimes(2);
	});
});
