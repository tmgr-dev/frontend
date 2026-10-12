import store from '@/store';
import { createLocalProfile, loadLocalProfile } from '@/utils/localProfile';
import { listLocalWorkspaces, localContext } from '../runtime';
import { importApi } from './api';
import { createMover, type Mover } from './flow';
import type { DecisionDeps, RemoveDeps } from './guards';
import { readInventory } from './inventory';
import { browserStorage } from './model';
import * as native from './native';

const reader = { id: 0, name: '', email: '' };

const localWorkspace = async (code: string) => {
	const workspace = (await listLocalWorkspaces()).find((w) => w.code === code);
	if (!workspace) throw new Error(`Local workspace ${code} not found`);
	return workspace;
};

export const currentUserId = (): number | null => {
	const id = Number(store.state.user?.id);
	return Number.isFinite(id) && id > 0 ? id : null;
};

export const userSnapshot = () => ({
	userId: currentUserId(),
	signedIn: !!store.state.token?.token,
});

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
			userId: currentUserId,
		});
	}
	return shared;
};

export const removeDeps: RemoveDeps = {
	storage: browserStorage,
	sourceRevision: native.sourceRevision,
	moveArchive: native.moveArchive,
	workspaceRemove: native.workspaceRemove,
	session: () => ({
		userId: currentUserId(),
		token: store.state.token?.token ?? null,
		apiBase: import.meta.env.VITE_API_BASE_URL,
	}),
};

export const decisionDeps: DecisionDeps = { storage: browserStorage, native };
