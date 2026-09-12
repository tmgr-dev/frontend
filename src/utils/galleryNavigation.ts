import type { TaskFile } from '@/actions/tmgr/files';
import { isImageMime } from '@/utils/attachments';

/**
 * Moves an index by one step, wrapping at both ends so the arrows never dead-end on the first or
 * last image. Returns 0 for an empty or single-item gallery, and for an index that is out of range.
 */
export const stepIndex = (
	current: number,
	length: number,
	delta: number,
): number => {
	if (length <= 1) {
		return 0;
	}
	if (current < 0 || current >= length) {
		return 0;
	}

	return (current + delta + length) % length;
};

/** The attachments the gallery can show, in the order the list shows them. */
export const galleryImages = (files: TaskFile[]): TaskFile[] =>
	files.filter((file) => isImageMime(file.mime_type));
