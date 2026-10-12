import { createLocalProfile, loadLocalProfile } from '@/utils/localProfile';
import { listLocalWorkspaces, localContext } from '../runtime';
import { importApi } from './api';
import { createMover, type Mover } from './flow';
import { readInventory } from './inventory';
import { browserStorage, type MoveStorage } from './model';
import * as native from './native';

const reader = { id: 0, name: '', email: '' };

const localWorkspace = async (code: string) => {
	const workspace = (await listLocalWorkspaces()).find((w) => w.code === code);
	if (!workspace) throw new Error(`Local workspace ${code} not found`);
	return workspace;
};

let shared: Mover | null = null;

export const getMover = (): Mover => {
	if (!shared) {
		shared = createMover({
			native,
			api: importApi(),
			storage: browserStorage,
			now: () => new Date(),
			sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
			profileUuid: async () =>
				((await loadLocalProfile()) ?? (await createLocalProfile())).uuid,
			inventory: async (code) =>
				readInventory(
					(await localContext(await localWorkspace(code), reader)).db,
				),
		});
	}
	return shared;
};

export const removeDeps = {
	storage: browserStorage as MoveStorage,
	sourceRevision: native.sourceRevision,
	workspaceRemove: native.workspaceRemove,
};

/** Saves one attachment of the local workspace through the browser's download flow; reading changes nothing. */
export const exportLocalFile = async (
	code: string,
	file: { key: string; name: string },
): Promise<void> => {
	const context = await localContext(await localWorkspace(code), reader);
	const blob = await context.files.read(file.key);
	const url = URL.createObjectURL(blob);
	const link = document.createElement('a');
	link.href = url;
	link.download = file.name;
	document.body.appendChild(link);
	link.click();
	link.remove();
	setTimeout(() => URL.revokeObjectURL(url), 10_000);
};
