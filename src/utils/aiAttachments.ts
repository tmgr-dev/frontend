export const MAX_ATTACHMENT_BYTES = 10 * 1024 * 1024;
export const MAX_ATTACHMENTS_PER_MESSAGE = 5;

export interface AttachedFile {
	id: number;
	name: string;
	mime_type: string | null;
	size: number | null;
}

const IMAGE_TYPES = ['image/png', 'image/jpeg', 'image/gif', 'image/webp'];

const DOCUMENT_TYPES = [
	'application/pdf',
	'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
	'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
	'application/vnd.openxmlformats-officedocument.presentationml.presentation',
	'application/json',
	'application/xml',
	'application/x-yaml',
	'application/yaml',
	'application/javascript',
	'application/x-sh',
	'application/sql',
	'application/toml',
];

const EXTENSIONS = [
	'pdf',
	'png',
	'jpg',
	'jpeg',
	'gif',
	'webp',
	'docx',
	'xlsx',
	'pptx',
	'txt',
	'text',
	'log',
	'md',
	'markdown',
	'csv',
	'tsv',
	'json',
	'java',
	'ts',
	'tsx',
	'js',
	'jsx',
	'mjs',
	'py',
	'go',
	'rb',
	'php',
	'kt',
	'sql',
	'yaml',
	'yml',
	'xml',
	'html',
	'css',
	'sh',
	'vue',
	'rs',
	'c',
	'h',
	'cpp',
	'cs',
	'swift',
	'toml',
	'ini',
	'properties',
	'gradle',
];

export const SUPPORTED_ATTACHMENTS_HINT =
	'PDF, images, Word, Excel, PowerPoint and text files';

export const ATTACHMENT_ACCEPT = [
	...EXTENSIONS.map((ext) => `.${ext}`),
	...IMAGE_TYPES,
	...DOCUMENT_TYPES,
	'text/*',
].join(',');

const extensionOf = (name: string): string => {
	const dot = name.lastIndexOf('.');
	return dot < 0 ? '' : name.slice(dot + 1).toLowerCase();
};

const baseType = (type: string | null | undefined): string =>
	(type || '').split(';', 1)[0].trim().toLowerCase();

export const isSupportedAttachment = (file: {
	name: string;
	type?: string | null;
}): boolean => {
	const type = baseType(file.type);
	if (
		type.startsWith('text/') ||
		IMAGE_TYPES.includes(type) ||
		DOCUMENT_TYPES.includes(type)
	) {
		return true;
	}
	return EXTENSIONS.includes(extensionOf(file.name));
};

export const isImageAttachment = (type: string | null | undefined): boolean =>
	baseType(type).startsWith('image/');

export const formatBytes = (bytes: number | null | undefined): string => {
	if (bytes == null || !Number.isFinite(bytes)) return '';
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
	return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
};

export const attachmentKindLabel = (
	name: string,
	type?: string | null,
): string => {
	const ext = extensionOf(name);
	if (ext) return ext.toUpperCase();
	const sub = baseType(type).split('/')[1];
	return sub ? sub.toUpperCase() : 'FILE';
};

export const validateAttachment = (
	file: { name: string; type?: string | null; size: number },
	alreadyAttached: number,
	maxFiles = MAX_ATTACHMENTS_PER_MESSAGE,
): string | null => {
	if (alreadyAttached >= maxFiles) {
		return `You can attach up to ${maxFiles} files to one message.`;
	}
	if (!isSupportedAttachment(file)) {
		return `${file.name}: this file type is not supported. Supported: ${SUPPORTED_ATTACHMENTS_HINT}.`;
	}
	if (file.size > MAX_ATTACHMENT_BYTES) {
		return `${file.name} is larger than ${formatBytes(MAX_ATTACHMENT_BYTES)}.`;
	}
	if (file.size === 0) {
		return `${file.name} is empty.`;
	}
	return null;
};

const ERROR_MESSAGES: Record<string, string> = {
	file_not_found: 'An attached file was not found. Remove it and try again.',
	forbidden: 'You don’t have access to one of the attached files.',
	file_not_on_task: 'Only files attached to this task can be used here.',
	file_too_large: `An attached file is larger than ${formatBytes(
		MAX_ATTACHMENT_BYTES,
	)}.`,
	too_many_files: `You can attach up to ${MAX_ATTACHMENTS_PER_MESSAGE} files to one message.`,
	unsupported_file_type: `One of the files can’t be read by the assistant. Supported: ${SUPPORTED_ATTACHMENTS_HINT}.`,
	too_many_pages: 'A PDF has too many pages (the limit is 40).',
	unreadable_file: 'One of the files could not be read.',
	documents_too_large:
		'The attached documents are too large to send together. Remove some and try again.',
	invalid_file: 'The file name is not valid.',
	invalid_file_path: 'The upload could not be attached. Try again.',
	file_bound_elsewhere: 'The upload could not be attached. Try again.',
};

export const attachmentErrorCode = (error: unknown): string | null => {
	const data = (error as { response?: { data?: { error?: unknown } } })
		?.response?.data;
	return typeof data?.error === 'string' ? data.error : null;
};

export const attachmentErrorMessage = (error: unknown): string | null => {
	const code = attachmentErrorCode(error);
	return code ? (ERROR_MESSAGES[code] ?? null) : null;
};

const pad = (n: number): string => String(n).padStart(2, '0');

export const withUniquePasteName = (file: File, now = new Date()): File => {
	if (file.name && !/^image\.(png|jpe?g|gif|webp)$/i.test(file.name)) {
		return file;
	}
	const ext =
		extensionOf(file.name) || baseType(file.type).split('/')[1] || 'png';
	const stamp = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(
		now.getDate(),
	)}-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
	const suffix = Math.random().toString(36).slice(2, 6);
	return new File([file], `screenshot-${stamp}-${suffix}.${ext}`, {
		type: file.type,
		lastModified: file.lastModified,
	});
};
