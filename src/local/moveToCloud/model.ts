import type { CreateJobRequest, PersonaMappingEntry } from './api';
import type { MoveSnapshotFile } from './native';

export const BUNDLE_SCHEMA = 1;

export type ExcludedReason = 'missing' | 'too_large' | 'mime' | 'refused';

export type FileResolution = 'exported' | 'lost';

export interface ExcludedFile {
	id: number;
	name: string;
	key: string;
	size: number;
	reason: ExcludedReason;
	resolution: FileResolution | null;
}

export type MoveStep =
	| 'started'
	| 'snapshotted'
	| 'built'
	| 'jobCreated'
	| 'bundleUploaded'
	| 'filesUploaded'
	| 'committed'
	| 'published';

export const STEP_ORDER: MoveStep[] = [
	'started',
	'snapshotted',
	'built',
	'jobCreated',
	'bundleUploaded',
	'filesUploaded',
	'committed',
	'published',
];

export const stepIndex = (step: MoveStep): number => STEP_ORDER.indexOf(step);

export interface MoveLimits {
	max_file_bytes: number;
	mime_allowlist: string[];
	bundle_schema: number;
}

export interface MoveInput {
	cloudName: string;
	personas: PersonaMappingEntry[];
}

export interface StagedFile {
	id: number;
	key: string;
	size: number;
	sha256: string;
	mime: string;
}

export interface MoveJob {
	step: MoveStep;
	localName: string;
	input: MoveInput;
	limits: MoveLimits;
	stoppedTimers: number;
	jobId: string | null;
	dir: string | null;
	revision: string | null;
	bundlePath: string | null;
	request: CreateJobRequest | null;
	staged: StagedFile[];
	excluded: ExcludedFile[];
	confirmed: string[];
	startedAt: string;
	error: string | null;
	userId: number | null;
}

export interface PublishedMove {
	jobId: string;
	workspaceId: number | null;
	name: string;
	at: string;
	revision: string;
	verified: boolean;
	excluded: ExcludedFile[];
	archive: { path: string; revision: string; fullDigest: string } | null;
	dir: string | null;
	userId: number | null;
}

export interface PersistedMove {
	v: 1;
	job: MoveJob | null;
	published: PublishedMove | null;
}

export interface MoveStorage {
	get(key: string): string | null;
	set(key: string, value: string): void;
	remove(key: string): void;
}

export const storageKey = (code: string) => `local.move.${code}`;

export const emptyPersisted = (): PersistedMove => ({
	v: 1,
	job: null,
	published: null,
});

export const loadPersisted = (
	storage: MoveStorage,
	code: string,
): PersistedMove => {
	try {
		const raw = storage.get(storageKey(code));
		if (!raw) return emptyPersisted();
		const parsed = JSON.parse(raw);
		if (parsed?.v !== 1) return emptyPersisted();
		return {
			v: 1,
			job: parsed.job ?? null,
			published: parsed.published ?? null,
		};
	} catch {
		return emptyPersisted();
	}
};

export const savePersisted = (
	storage: MoveStorage,
	code: string,
	state: PersistedMove,
): void => {
	try {
		if (!state.job && !state.published) storage.remove(storageKey(code));
		else storage.set(storageKey(code), JSON.stringify(state));
	} catch {
		/* storage unavailable: progress lasts until the window closes */
	}
};

export const browserStorage: MoveStorage = {
	get: (key) => localStorage.getItem(key),
	set: (key, value) => localStorage.setItem(key, value),
	remove: (key) => localStorage.removeItem(key),
};

const baseMime = (mime: string) => mime.split(';')[0].trim().toLowerCase();

export const mimeAllowed = (mime: string, allowlist: string[]): boolean => {
	if (!allowlist.length) return true;
	const type = baseMime(mime);
	return allowlist.some((entry) => {
		const allowed = baseMime(entry);
		return allowed.endsWith('*')
			? type.startsWith(allowed.slice(0, -1))
			: allowed === type;
	});
};

const EXTENSION_MIME: Record<string, string> = {
	png: 'image/png',
	jpg: 'image/jpeg',
	jpeg: 'image/jpeg',
	gif: 'image/gif',
	webp: 'image/webp',
	svg: 'image/svg+xml',
	pdf: 'application/pdf',
	txt: 'text/plain',
	log: 'text/plain',
	md: 'text/markdown',
	json: 'application/json',
	csv: 'text/csv',
	zip: 'application/zip',
	mp4: 'video/mp4',
	mov: 'video/quicktime',
};

/** Same fallback the Rust side uses when a file row has no MIME type. */
export const resolveMime = (
	rowMime: string | null | undefined,
	key: string,
): string => {
	const own = rowMime ? baseMime(rowMime) : '';
	if (own) return own;
	const extension = key.includes('.')
		? key.split('.').pop()!.toLowerCase()
		: '';
	return EXTENSION_MIME[extension] ?? 'application/octet-stream';
};

export const excludedReason = (
	file: { size: number; mime: string; missing?: boolean },
	limits: Pick<MoveLimits, 'max_file_bytes' | 'mime_allowlist'>,
): ExcludedReason | null => {
	if (file.missing) return 'missing';
	if (file.size > limits.max_file_bytes) return 'too_large';
	if (!mimeAllowed(file.mime, limits.mime_allowlist)) return 'mime';
	return null;
};

export const reasonLabel = (reason: ExcludedReason): string =>
	({
		missing: 'file is missing',
		too_large: 'too large',
		mime: 'type not allowed',
		refused: 'refused by the server',
	}[reason]);

export const splitSnapshotFiles = (
	files: MoveSnapshotFile[],
	names: Map<number, string>,
	limits: MoveLimits,
): { staged: StagedFile[]; excluded: ExcludedFile[] } => {
	const staged: StagedFile[] = [];
	const excluded: ExcludedFile[] = [];
	for (const file of files) {
		const reason = excludedReason(file, limits);
		if (reason) {
			excluded.push({
				id: file.id,
				name: names.get(file.id) ?? file.key,
				key: file.key,
				size: file.size,
				reason,
				resolution: null,
			});
		} else {
			staged.push({
				id: file.id,
				key: file.key,
				size: file.size,
				sha256: file.sha256,
				mime: file.mime,
			});
		}
	}
	return { staged, excluded };
};
