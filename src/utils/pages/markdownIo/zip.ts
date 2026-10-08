import { Unzip, UnzipInflate, UnzipPassThrough, zipSync } from 'fflate';
import { isHiddenPath, safeEntryPath } from './paths';
import {
	IMPORT_LIMITS,
	type ImportLimits,
	type IoWarning,
	type ReadFilesResult,
	type VirtualFile,
} from './types';

const CHUNK = 16 * 1024;
const STORED_EXT = /\.(png|jpe?g|gif|webp|avif|zip|pdf|gz|mp4|mov|mp3)$/i;

export const writeZip = (files: VirtualFile[]): Uint8Array => {
	const entries: Record<string, [Uint8Array, { level: 0 | 6 }]> = {};
	for (const file of files) {
		entries[file.path] = [
			file.bytes,
			{ level: STORED_EXT.test(file.path) ? 0 : 6 },
		];
	}
	return zipSync(entries);
};

export const readZip = (
	bytes: Uint8Array,
	limits: ImportLimits = IMPORT_LIMITS,
	entryBudget: number = limits.maxEntries,
): ReadFilesResult => {
	const files: VirtualFile[] = [];
	const warnings: IoWarning[] = [];
	let entries = 0;
	let total = 0;
	let failure: string | null = null;

	const unzip = new Unzip();
	unzip.register(UnzipInflate);
	unzip.register(UnzipPassThrough);
	unzip.onfile = (entry) => {
		if (failure) {
			entry.terminate();
			return;
		}
		if (entry.name.endsWith('/')) return;
		entries += 1;
		if (entries > entryBudget) {
			failure = `The archive has more than ${limits.maxEntries} files`;
			entry.terminate();
			return;
		}
		const path = safeEntryPath(entry.name);
		const skip = path === null || isHiddenPath(path);
		if (path === null) {
			warnings.push({
				path: entry.name.replace(/[^\x20-\x7e]/g, '?'),
				message: 'Skipped an entry with an unsafe path',
			});
		}
		const chunks: Uint8Array[] = [];
		let size = 0;
		let tooBig = false;
		entry.ondata = (error, chunk, final) => {
			if (error || failure) {
				if (error && !failure) failure = 'The archive is damaged';
				return;
			}
			total += chunk.length;
			if (total > limits.maxTotalBytes) {
				failure = `The archive unpacks to more than ${Math.round(
					limits.maxTotalBytes / 1024 / 1024,
				)} MB`;
				entry.terminate();
				return;
			}
			if (skip || tooBig) return;
			size += chunk.length;
			if (size > limits.maxEntryBytes) {
				tooBig = true;
				chunks.length = 0;
				warnings.push({
					path,
					message: `Skipped a file larger than ${Math.round(
						limits.maxEntryBytes / 1024 / 1024,
					)} MB`,
				});
				return;
			}
			chunks.push(chunk);
			if (final) {
				const merged = new Uint8Array(size);
				let offset = 0;
				for (const part of chunks) {
					merged.set(part, offset);
					offset += part.length;
				}
				files.push({ path: path as string, bytes: merged });
			}
		};
		entry.start();
	};

	try {
		for (let offset = 0; offset < bytes.length && !failure; offset += CHUNK) {
			const end = Math.min(offset + CHUNK, bytes.length);
			unzip.push(bytes.subarray(offset, end), end === bytes.length);
		}
		if (!bytes.length) failure = 'The archive is empty';
	} catch {
		failure = 'The archive could not be read';
	}
	if (!failure && entries === 0) failure = 'The archive has no readable files';
	if (failure) {
		return { files: [], warnings: [{ path: null, message: failure }] };
	}
	return { files, warnings };
};

export interface RawInput {
	name: string;
	bytes: Uint8Array;
}

const looksLikeZip = (input: RawInput): boolean =>
	/\.zip$/i.test(input.name) ||
	(input.bytes.length > 3 &&
		input.bytes[0] === 0x50 &&
		input.bytes[1] === 0x4b &&
		input.bytes[2] === 0x03 &&
		input.bytes[3] === 0x04);

export const readVirtualFiles = (
	inputs: RawInput[],
	limits: ImportLimits = IMPORT_LIMITS,
): ReadFilesResult => {
	const files: VirtualFile[] = [];
	const warnings: IoWarning[] = [];
	const input = inputs.reduce((sum, item) => sum + item.bytes.length, 0);
	if (input > limits.maxInputBytes) {
		return {
			files: [],
			warnings: [
				{
					path: null,
					message: `The selected files are larger than ${Math.round(
						limits.maxInputBytes / 1024 / 1024,
					)} MB`,
				},
			],
		};
	}
	for (const item of inputs) {
		if (looksLikeZip(item)) {
			const result = readZip(
				item.bytes,
				limits,
				limits.maxEntries - files.length,
			);
			files.push(...result.files);
			warnings.push(
				...result.warnings.map((warning) => ({
					...warning,
					message: warning.path
						? warning.message
						: `${item.name}: ${warning.message}`,
				})),
			);
			continue;
		}
		const path = safeEntryPath(item.name);
		if (path === null) {
			warnings.push({
				path: null,
				message: 'Skipped a file with an unsafe name',
			});
			continue;
		}
		if (isHiddenPath(path)) continue;
		if (item.bytes.length > limits.maxEntryBytes) {
			warnings.push({
				path,
				message: `Skipped a file larger than ${Math.round(
					limits.maxEntryBytes / 1024 / 1024,
				)} MB`,
			});
			continue;
		}
		files.push({ path, bytes: item.bytes });
	}
	if (files.length > limits.maxEntries) {
		return {
			files: [],
			warnings: [
				{
					path: null,
					message: `More than ${limits.maxEntries} files were selected`,
				},
			],
		};
	}
	return { files, warnings };
};
