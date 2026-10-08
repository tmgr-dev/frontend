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

export interface ReadBudget {
	entries: number;
	bytes: number;
}

export const readZip = (
	bytes: Uint8Array,
	limits: ImportLimits = IMPORT_LIMITS,
	budget: ReadBudget = { entries: 0, bytes: 0 },
): ReadFilesResult => {
	const files: VirtualFile[] = [];
	const warnings: IoWarning[] = [];
	let failure: string | null = null;
	let seen = 0;

	const unzip = new Unzip();
	unzip.register(UnzipInflate);
	unzip.register(UnzipPassThrough);
	unzip.onfile = (entry) => {
		if (failure) {
			entry.terminate();
			return;
		}
		budget.entries += 1;
		if (budget.entries > limits.maxEntries) {
			failure = `The archive has more than ${limits.maxEntries} files`;
			entry.terminate();
			return;
		}
		if (entry.name.endsWith('/')) return;
		seen += 1;
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
			budget.bytes += chunk.length;
			if (budget.bytes > limits.maxTotalBytes) {
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
	if (!failure && seen === 0) failure = 'The archive has no readable files';
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
	const budget: ReadBudget = { entries: 0, bytes: 0 };
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
	const seen = new Map<string, string>();
	const add = (file: VirtualFile) => {
		const key = file.path.toLowerCase();
		if (seen.has(key)) {
			warnings.push({
				path: file.path,
				message: `Skipped: the path differs only by case from ${seen.get(key)}`,
			});
			return;
		}
		seen.set(key, file.path);
		files.push(file);
	};
	for (const item of inputs) {
		if (looksLikeZip(item)) {
			const result = readZip(item.bytes, limits, budget);
			result.files.forEach(add);
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
		budget.entries += 1;
		budget.bytes += item.bytes.length;
		if (budget.entries > limits.maxEntries) {
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
		add({ path, bytes: item.bytes });
	}
	return { files, warnings };
};

export interface RawFile {
	name: string;
	size: number;
	webkitRelativePath?: string;
	arrayBuffer(): Promise<ArrayBuffer>;
}

export const readRawFiles = async (
	files: RawFile[],
	limits: ImportLimits = IMPORT_LIMITS,
): Promise<ReadFilesResult> => {
	if (files.length > limits.maxEntries) {
		throw new Error(`More than ${limits.maxEntries} files were selected`);
	}
	if (files.reduce((sum, file) => sum + file.size, 0) > limits.maxInputBytes) {
		throw new Error(
			`The selected files are larger than ${Math.round(
				limits.maxInputBytes / 1024 / 1024,
			)} MB`,
		);
	}
	const inputs: RawInput[] = [];
	const warnings: IoWarning[] = [];
	for (const file of files) {
		const name = file.webkitRelativePath || file.name;
		try {
			inputs.push({ name, bytes: new Uint8Array(await file.arrayBuffer()) });
		} catch {
			warnings.push({
				path: name,
				message: 'Skipped: the file could not be read',
			});
		}
	}
	const result = readVirtualFiles(inputs, limits);
	return { files: result.files, warnings: [...warnings, ...result.warnings] };
};
