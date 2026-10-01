export const formatFileSize = (bytes: number | null | undefined): string => {
	if (bytes === null || bytes === undefined || bytes < 0) return '';
	if (bytes < 1024) return `${bytes} Б`;
	if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} КБ`;
	return `${(bytes / (1024 * 1024)).toFixed(1)} МБ`;
};

export const imageFilesOf = (
	files: ArrayLike<File> | null | undefined,
): File[] =>
	Array.from(files ?? []).filter((file) => file.type.startsWith('image/'));

export const fileMarkdown = (id: number, alt = ''): string =>
	`![${alt.replace(/([[\]])/g, '\\$1')}](tmgr://file/${id})`;
