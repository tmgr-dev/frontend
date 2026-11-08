import { t } from '@/i18n';

export const formatFileSize = (bytes: number | null | undefined): string => {
	if (bytes === null || bytes === undefined || bytes < 0) return '';
	if (bytes < 1024) return t('tasks.helpers.fileSize.b', { n: bytes });
	if (bytes < 1024 * 1024)
		return t('tasks.helpers.fileSize.kb', { n: Math.round(bytes / 1024) });
	return t('tasks.helpers.fileSize.mb', {
		n: (bytes / (1024 * 1024)).toFixed(1),
	});
};

export const imageFilesOf = (
	files: ArrayLike<File> | null | undefined,
): File[] =>
	Array.from(files ?? []).filter((file) => file.type.startsWith('image/'));

export const fileMarkdown = (id: number, alt = ''): string =>
	`![${alt.replace(/([[\]])/g, '\\$1')}](tmgr://file/${id})`;
