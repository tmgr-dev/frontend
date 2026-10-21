import type { AttachedFile } from '@/utils/aiAttachments';
import {
	MAX_ATTACHMENTS_PER_MESSAGE,
	attachmentErrorMessage,
	validateAttachment,
	withUniquePasteName,
} from '@/utils/aiAttachments';
import { computed, ref } from 'vue';

export type AttachmentStatus = 'uploading' | 'ready' | 'error';

export interface AttachmentItem {
	key: string;
	name: string;
	size: number;
	mimeType: string;
	progress: number;
	status: AttachmentStatus;
	error: string | null;
	file: AttachedFile | null;
}

export type AttachmentUploader = (
	file: File,
	onProgress: (percent: number) => void,
) => Promise<AttachedFile>;

let sequence = 0;

export function useAttachmentQueue(
	uploader: AttachmentUploader,
	maxFiles = MAX_ATTACHMENTS_PER_MESSAGE,
) {
	const items = ref<AttachmentItem[]>([]);
	const notice = ref<string | null>(null);

	const find = (key: string) => items.value.find((i) => i.key === key);

	const uploadOne = async (key: string, file: File): Promise<void> => {
		try {
			const uploaded = await uploader(file, (percent) => {
				const item = find(key);
				if (item) item.progress = Math.max(0, Math.min(100, percent));
			});
			const item = find(key);
			if (!item) return;
			item.file = uploaded;
			item.progress = 100;
			item.status = 'ready';
		} catch (error) {
			const item = find(key);
			if (!item) return;
			item.status = 'error';
			item.error = attachmentErrorMessage(error) ?? 'Upload failed.';
		}
	};

	const add = (files: Iterable<File>): Promise<void> => {
		notice.value = null;
		const uploads: Promise<void>[] = [];
		for (const raw of files) {
			const file = withUniquePasteName(raw);
			const problem = validateAttachment(file, items.value.length, maxFiles);
			if (problem) {
				notice.value = problem;
				continue;
			}
			const key = `att-${++sequence}`;
			items.value.push({
				key,
				name: file.name,
				size: file.size,
				mimeType: file.type,
				progress: 0,
				status: 'uploading',
				error: null,
				file: null,
			});
			uploads.push(uploadOne(key, file));
		}
		return Promise.all(uploads).then(() => undefined);
	};

	const remove = (key: string): void => {
		items.value = items.value.filter((i) => i.key !== key);
		notice.value = null;
	};

	const clear = (): void => {
		items.value = [];
		notice.value = null;
	};

	const uploading = computed(() =>
		items.value.some((i) => i.status === 'uploading'),
	);
	const hasErrors = computed(() =>
		items.value.some((i) => i.status === 'error'),
	);
	const fileIds = computed(() =>
		items.value
			.filter((i) => i.status === 'ready' && i.file)
			.map((i) => (i.file as AttachedFile).id),
	);
	const readyFiles = computed(() =>
		items.value
			.filter((i) => i.status === 'ready' && i.file)
			.map((i) => i.file as AttachedFile),
	);
	const canSend = computed(() => !uploading.value && !hasErrors.value);

	return {
		items,
		notice,
		add,
		remove,
		clear,
		uploading,
		hasErrors,
		fileIds,
		readyFiles,
		canSend,
	};
}
