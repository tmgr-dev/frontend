import { currentLocale, t } from '@/i18n';

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

export const supportedAttachmentsHint = (): string =>
	t('agents.helpers.attachments.hint');

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

const formatNumber = (value: number, fractionDigits = 0): string =>
	new Intl.NumberFormat(currentLocale(), {
		minimumFractionDigits: fractionDigits,
		maximumFractionDigits: fractionDigits,
		useGrouping: false,
	}).format(value);

export const formatBytes = (bytes: number | null | undefined): string => {
	if (bytes == null || !Number.isFinite(bytes)) return '';
	if (bytes < 1024)
		return t('agents.helpers.attachments.size.bytes', {
			n: formatNumber(bytes),
		});
	if (bytes < 1024 * 1024)
		return t('agents.helpers.attachments.size.kilobytes', {
			n: formatNumber(Math.round(bytes / 1024)),
		});
	return t('agents.helpers.attachments.size.megabytes', {
		n: formatNumber(bytes / (1024 * 1024), 1),
	});
};

export const attachmentKindLabel = (
	name: string,
	type?: string | null,
): string => {
	const ext = extensionOf(name);
	if (ext) return ext.toUpperCase();
	const sub = baseType(type).split('/')[1];
	return sub ? sub.toUpperCase() : t('agents.helpers.attachments.file');
};

export const validateAttachment = (
	file: { name: string; type?: string | null; size: number },
	alreadyAttached: number,
	maxFiles = MAX_ATTACHMENTS_PER_MESSAGE,
): string | null => {
	if (alreadyAttached >= maxFiles) {
		return t('agents.helpers.attachments.tooMany', { max: maxFiles });
	}
	if (!isSupportedAttachment(file)) {
		return t('agents.helpers.attachments.unsupported', {
			name: file.name,
			hint: supportedAttachmentsHint(),
		});
	}
	if (file.size > MAX_ATTACHMENT_BYTES) {
		return t('agents.helpers.attachments.tooLarge', {
			name: file.name,
			size: formatBytes(MAX_ATTACHMENT_BYTES),
		});
	}
	if (file.size === 0) {
		return t('agents.helpers.attachments.empty', { name: file.name });
	}
	return null;
};

const ERROR_CODES = new Set([
	'file_not_found',
	'forbidden',
	'file_not_on_task',
	'file_too_large',
	'too_many_files',
	'unsupported_file_type',
	'too_many_pages',
	'unreadable_file',
	'documents_too_large',
	'invalid_file',
	'invalid_file_path',
	'file_bound_elsewhere',
]);

export const attachmentErrorCode = (error: unknown): string | null => {
	const data = (error as { response?: { data?: { error?: unknown } } })
		?.response?.data;
	return typeof data?.error === 'string' ? data.error : null;
};

export const attachmentErrorMessage = (error: unknown): string | null => {
	const code = attachmentErrorCode(error);
	return code && ERROR_CODES.has(code)
		? t(`agents.helpers.attachments.errors.${code}`, {
				size: formatBytes(MAX_ATTACHMENT_BYTES),
				max: MAX_ATTACHMENTS_PER_MESSAGE,
				hint: supportedAttachmentsHint(),
			})
		: null;
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
