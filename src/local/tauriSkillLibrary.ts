import {
	emptyLibrary,
	validateLibraryReport,
	type LibraryFile,
	type LibraryReport,
	type LibrarySkill,
} from './skillLibrary';
import { bumpSkillLibraryRevision } from './skillLibraryRevision';

const invoke = async <T>(
	command: string,
	args?: Record<string, unknown>,
): Promise<T> => {
	const core = await import('@tauri-apps/api/core');
	return core.invoke<T>(command, args);
};

export const readSkillLibraryReport = async (
	userId: number,
): Promise<LibraryReport> => {
	const raw = await invoke<unknown>('skill_library_read', { userId });
	return raw == null
		? { library: emptyLibrary(), skipped: [] }
		: validateLibraryReport(raw);
};

export const readSkillLibrary = async (userId: number): Promise<LibraryFile> =>
	(await readSkillLibraryReport(userId)).library;

export const writeSkillLibrary = (
	userId: number,
	data: LibraryFile,
): Promise<void> => invoke('skill_library_write', { userId, data });

let queue: Promise<unknown> = Promise.resolve();

/** Every write is a read-modify-write of the latest file, one at a time, so the editor, the import dialog and the MCP never overwrite each other. */
export const updateSkillLibrary = <T>(
	userId: number,
	change: (lib: LibraryFile) => { library: LibraryFile; result: T },
): Promise<T> => {
	const run = queue.then(async () => {
		const { library, result } = change(await readSkillLibrary(userId));
		await writeSkillLibrary(userId, library);
		bumpSkillLibraryRevision();
		return result;
	});
	queue = run.catch(() => undefined);
	return run;
};

export const exportSkillLibrary = (userId: number): Promise<boolean> =>
	invoke('skill_library_export', { userId });

export const pickSkillLibraryImport = (): Promise<{
	version: 1;
	skills: LibrarySkill[];
} | null> => invoke('skill_library_import_pick');
