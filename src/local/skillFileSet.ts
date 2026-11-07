import { byteLength } from '@/utils/personas';

export interface SkillFileEntry {
	path: string;
	content: string;
	size: number;
	sha256: string;
}

export interface SkillFileSet {
	files: SkillFileEntry[];
	sha256: string;
}

export const EMPTY_FILES_SHA256 =
	'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855';

export const SKILL_FILE_MAX_BYTES = 64 * 1024;
export const SKILL_FILES_MAX = 50;
export const SKILL_FILES_MAX_TOTAL_BYTES = 256 * 1024;

const MAX_PATH_CHARS = 200;
const MAX_SEGMENTS = 5;
const SEGMENT = /^[A-Za-z0-9_][A-Za-z0-9._-]*$/;
const SHA256 = /^[0-9a-f]{64}$/;

export const isValidFilePath = (path: unknown): path is string => {
	if (typeof path !== 'string' || !path || path.length > MAX_PATH_CHARS) {
		return false;
	}
	const segments = path.split('/');
	return (
		segments.length <= MAX_SEGMENTS &&
		segments.every((segment) => SEGMENT.test(segment))
	);
};

export const checkSkillFiles = (
	files: unknown,
	sha256: unknown,
): { ok: true; set: SkillFileSet } | { ok: false; reason: string } => {
	const fail = (reason: string) => ({ ok: false as const, reason });
	if (!Array.isArray(files)) return fail('files must be a list');
	if (files.length > SKILL_FILES_MAX) {
		return fail(`a skill has at most ${SKILL_FILES_MAX} files`);
	}
	if (typeof sha256 !== 'string' || !SHA256.test(sha256)) {
		return fail('files hash is missing');
	}
	const keys = new Set<string>();
	const entries: SkillFileEntry[] = [];
	let total = 0;
	for (const file of files) {
		if (!file || typeof file !== 'object') return fail('file is malformed');
		const { path, content, size, sha256: fileSha } = file as SkillFileEntry;
		if (!isValidFilePath(path)) return fail('invalid file path');
		const key = path.toLowerCase();
		if (key === 'skill.md') return fail('SKILL.md is reserved');
		if (keys.has(key))
			return fail(`${path} differs from another file only by case`);
		keys.add(key);
		if (typeof content !== 'string' || content.includes('\0')) {
			return fail(`${path} is not text`);
		}
		const bytes = byteLength(content);
		if (bytes > SKILL_FILE_MAX_BYTES) {
			return fail(`${path} is larger than ${SKILL_FILE_MAX_BYTES / 1024} KiB`);
		}
		if (
			size !== bytes ||
			typeof fileSha !== 'string' ||
			!SHA256.test(fileSha)
		) {
			return fail(`${path} does not match its size or hash`);
		}
		total += bytes;
		entries.push({ path, content, size, sha256: fileSha });
	}
	if (total > SKILL_FILES_MAX_TOTAL_BYTES) {
		return fail(
			`the files of a skill are limited to ${SKILL_FILES_MAX_TOTAL_BYTES / 1024} KiB in total`,
		);
	}
	return { ok: true, set: { files: entries, sha256 } };
};
