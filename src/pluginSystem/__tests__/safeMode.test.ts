import { pushSafeMode, syncStoredSafeMode } from '../safeMode';

describe('pushSafeMode', () => {
	it('tells the desktop app and reports success', async () => {
		const calls: unknown[][] = [];
		const ok = await pushSafeMode(true, async (...args) => {
			calls.push(args);
		});
		expect(ok).toBe(true);
		expect(calls).toEqual([['plugins_set_safe_mode', { value: true }]]);
	});

	it('reports failure when the desktop app refuses', async () => {
		const ok = await pushSafeMode(false, async () => {
			throw new Error('denied');
		});
		expect(ok).toBe(false);
	});
});

describe('syncStoredSafeMode', () => {
	it('pushes a stored switch to the desktop app when it does not know it yet', async () => {
		const calls: unknown[][] = [];
		await syncStoredSafeMode(true, false, async (...args) => {
			calls.push(args);
		});
		expect(calls).toEqual([['plugins_set_safe_mode', { value: true }]]);
	});

	it('does nothing when the switch is off or the desktop app already has it on', async () => {
		const invoke = jest.fn();
		await syncStoredSafeMode(false, false, invoke);
		await syncStoredSafeMode(true, true, invoke);
		expect(invoke).not.toHaveBeenCalled();
	});
});
