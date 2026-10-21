jest.mock('@/actions/tmgr/files', () => ({
	presignUpload: jest.fn(),
	putToStorageWithProgress: jest.fn(),
	attachFile: jest.fn(),
}));

import {
	attachFile,
	presignUpload,
	putToStorageWithProgress,
} from '@/actions/tmgr/files';
import { uploadCommentAttachment } from '../commentAttachments';

const target = {
	key: 'k',
	upload_url: 'u',
	method: 'PUT',
	content_type: 'text/plain',
	max_bytes: 1,
};

describe('uploadCommentAttachment', () => {
	beforeEach(() => jest.clearAllMocks());

	it('runs presign, PUT and attach and maps the saved file', async () => {
		const file = new File(['abc'], 'a.txt', { type: 'text/plain' });
		const onProgress = jest.fn();
		(presignUpload as jest.Mock).mockResolvedValue(target);
		(putToStorageWithProgress as jest.Mock).mockResolvedValue(undefined);
		(attachFile as jest.Mock).mockResolvedValue({
			id: 5,
			name: 'stored.txt',
			original_name: 'a.txt',
			mime_type: 'text/plain',
			size: 3,
		});

		const result = await uploadCommentAttachment(11, file, onProgress);

		expect(putToStorageWithProgress).toHaveBeenCalledWith(
			target,
			file,
			onProgress,
		);
		expect(attachFile).toHaveBeenCalledWith(11, file, target);
		expect(result).toEqual({
			id: 5,
			name: 'a.txt',
			mime_type: 'text/plain',
			size: 3,
		});
	});

	it('falls back to name when original_name is missing', async () => {
		(presignUpload as jest.Mock).mockResolvedValue(target);
		(putToStorageWithProgress as jest.Mock).mockResolvedValue(undefined);
		(attachFile as jest.Mock).mockResolvedValue({
			id: 6,
			name: 'n.txt',
			original_name: null,
			mime_type: null,
			size: null,
		});
		const result = await uploadCommentAttachment(
			1,
			new File(['x'], 'n.txt'),
			jest.fn(),
		);
		expect(result.name).toBe('n.txt');
	});

	it('propagates a failed step', async () => {
		(presignUpload as jest.Mock).mockRejectedValue(new Error('boom'));
		await expect(
			uploadCommentAttachment(1, new File(['x'], 'n.txt'), jest.fn()),
		).rejects.toThrow('boom');
		expect(attachFile).not.toHaveBeenCalled();
	});
});
