import { uploadPendingFiles } from '../pendingUploads';

const file = (name: string) => ({ name }) as File;

describe('uploadPendingFiles', () => {
	it('uploads every queued file', async () => {
		const result = await uploadPendingFiles(
			[file('a.png'), file('b.pdf')],
			async (f) => `attached:${f.name}`,
		);
		expect(result).toEqual({
			attached: ['attached:a.png', 'attached:b.pdf'],
			failed: [],
		});
	});

	it('keeps going when one upload fails and reports the failed file', async () => {
		const bad = file('big.zip');
		const result = await uploadPendingFiles(
			[file('a.png'), bad, file('c.txt')],
			async (f) => {
				if (f === bad) throw new Error('413');
				return f.name;
			},
		);
		expect(result.attached).toEqual(['a.png', 'c.txt']);
		expect(result.failed).toEqual([bad]);
	});

	it('does nothing for an empty queue', async () => {
		const upload = jest.fn();
		expect(await uploadPendingFiles([], upload)).toEqual({
			attached: [],
			failed: [],
		});
		expect(upload).not.toHaveBeenCalled();
	});
});
