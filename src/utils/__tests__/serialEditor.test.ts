import { createSerialEditor } from '../serialEditor';
it('coalesces concurrent edits and flushes the newest serialization', async () => {
	let release!: (value: string) => void;
	const save = jest.fn(
		() =>
			new Promise<string>((resolve) => {
				release = resolve;
			}),
	);
	const publish = jest.fn();
	const editor = createSerialEditor(save, publish);
	void editor.changed();
	void editor.changed();
	void editor.changed();
	expect(save).toHaveBeenCalledTimes(1);
	release('old');
	await Promise.resolve();
	await Promise.resolve();
	expect(save).toHaveBeenCalledTimes(2);
	release('new');
	expect(await editor.flush()).toBe('new');
	expect(publish).toHaveBeenLastCalledWith('new');
});

it('serializes document render, save and destruction even after an operation fails', async () => {
	const { createEditorOperationQueue } = await import('../serialEditor');
	const queue = createEditorOperationQueue();
	let release!: () => void;
	const calls: string[] = [];
	const render = queue(
		() =>
			new Promise<void>((resolve) => {
				calls.push('render');
				release = resolve;
			}),
	);
	const save = queue(async () => {
		calls.push('save');
		throw new Error('save failed');
	});
	const failure = expect(save).rejects.toThrow('save failed');
	const destroy = queue(async () => {
		calls.push('destroy');
	});
	await Promise.resolve();
	expect(calls).toEqual(['render']);
	release();
	await render;
	await failure;
	await destroy;
	expect(calls).toEqual(['render', 'save', 'destroy']);
});
