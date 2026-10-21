import {
	attachFile,
	presignUpload,
	putToStorageWithProgress,
} from '@/actions/tmgr/files';
import type { AttachedFile } from '@/utils/aiAttachments';

export const uploadCommentAttachment = async (
	taskId: number,
	file: File,
	onProgress: (percent: number) => void,
): Promise<AttachedFile> => {
	const target = await presignUpload(file);
	await putToStorageWithProgress(target, file, onProgress);
	const saved = await attachFile(taskId, file, target);
	return {
		id: saved.id,
		name: saved.original_name || saved.name,
		mime_type: saved.mime_type,
		size: saved.size,
	};
};

export const filesFromTransfer = (
	data: DataTransfer | null | undefined,
): File[] => (data?.files ? Array.from(data.files) : []);
