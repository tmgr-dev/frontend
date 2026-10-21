import { useAttachmentQueue } from '@/composable/useAttachmentQueue';

const pdf = (name = 'a.pdf', size = 3) =>
	new File(['x'.repeat(size)], name, { type: 'application/pdf' });

describe('useAttachmentQueue', () => {
	it('uploads files, reports progress and exposes ready ids', async () => {
		const seen: number[] = [];
		const queue = useAttachmentQueue(async (file, onProgress) => {
			onProgress(50);
			seen.push(queue.items.value[0].progress);
			return { id: 7, name: file.name, mime_type: file.type, size: file.size };
		});
		await queue.add([pdf()]);
		expect(seen).toEqual([50]);
		expect(queue.fileIds.value).toEqual([7]);
		expect(queue.items.value[0].status).toBe('ready');
		expect(queue.canSend.value).toBe(true);
	});

	it('keeps a failed upload as an error chip and blocks sending', async () => {
		const queue = useAttachmentQueue(async () => {
			throw { response: { data: { error: 'unsupported_file_type' } } };
		});
		await queue.add([pdf()]);
		expect(queue.items.value[0].status).toBe('error');
		expect(queue.items.value[0].error).toMatch(/can’t be read/);
		expect(queue.canSend.value).toBe(false);
		queue.remove(queue.items.value[0].key);
		expect(queue.canSend.value).toBe(true);
	});

	it('refuses unsupported files before uploading', async () => {
		const uploader = jest.fn();
		const queue = useAttachmentQueue(uploader);
		await queue.add([new File(['x'], 'a.zip', { type: 'application/zip' })]);
		expect(uploader).not.toHaveBeenCalled();
		expect(queue.items.value).toHaveLength(0);
		expect(queue.notice.value).toMatch(/not supported/);
	});

	it('stops at the per-message limit', async () => {
		const queue = useAttachmentQueue(
			async (file) => ({ id: 1, name: file.name, mime_type: null, size: 1 }),
			2,
		);
		await queue.add([pdf('1.pdf'), pdf('2.pdf'), pdf('3.pdf')]);
		expect(queue.items.value).toHaveLength(2);
		expect(queue.notice.value).toMatch(/up to 2 files/);
	});
});
