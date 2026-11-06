import { requestGhostPlay, stopGhost } from '@/onboarding/ghost';
import { FIRST_WORKSPACE_NAME } from './accountless';
import { DEMO_WORKSPACE_NAME, seedDemo } from './demoSeed';
import {
	activeLocalWorkspaceCode,
	createLocalWorkspace,
	firstRealWorkspace,
	forgetLocalWorkspace,
	listLocalWorkspaces,
	localContext,
	setActiveLocalWorkspace,
} from './runtime';
import { LOCAL_CODE_PREFIX, type LocalWorkspace } from './types';

const invoke = async <T>(
	command: string,
	args?: Record<string, unknown>,
): Promise<T> => {
	const core = await import('@tauri-apps/api/core');
	return core.invoke<T>(command, args);
};

export const findDemoWorkspace = async (
	refresh = false,
): Promise<LocalWorkspace | null> =>
	(await listLocalWorkspaces(refresh)).find((w) => w.demo) ?? null;

let creating: Promise<LocalWorkspace> | null = null;

const createOnce = async (): Promise<LocalWorkspace> => {
	const existing = await findDemoWorkspace();
	if (existing) return existing;
	await invoke('local_workspace_create_demo', { name: DEMO_WORKSPACE_NAME });
	const created = await findDemoWorkspace(true);
	if (!created) throw new Error('The demo workspace was not created');
	const ctx = await localContext(created, { id: 0, name: '', email: '' });
	await seedDemo(ctx);
	return created;
};

export const createDemoWorkspace = (): Promise<LocalWorkspace> =>
	(creating ??= createOnce().finally(() => {
		creating = null;
	}));

const reloadWorkspaces = async () => {
	const [{ requestCache }, { default: store }] = await Promise.all([
		import('@/utils/requestCache'),
		import('@/store'),
	]);
	requestCache.invalidate('workspaces');
	await store.dispatch('loadWorkspaces');
};

const openWorkspace = async (workspace: LocalWorkspace) => {
	setActiveLocalWorkspace(workspace);
	await reloadWorkspaces();
	const { default: router } = await import('@/router');
	await router.push(`/${LOCAL_CODE_PREFIX}${workspace.code}/board`);
};

export const watchDemo = async (): Promise<LocalWorkspace> => {
	const demo = await createDemoWorkspace();
	await openWorkspace(demo);
	requestGhostPlay();
	return demo;
};

export const deleteDemo = async (code: string): Promise<void> => {
	const list = await listLocalWorkspaces(true);
	if (!list.find((w) => w.code === code)?.demo)
		throw new Error('Only the demo workspace can be deleted this way');
	const wasActive = activeLocalWorkspaceCode() === code;
	await stopGhost();
	await invoke('local_workspace_delete_demo', { code });
	forgetLocalWorkspace(code);
	const rest = await listLocalWorkspaces(true);
	if (!wasActive) {
		await reloadWorkspaces();
		return;
	}
	const next =
		firstRealWorkspace(rest) ??
		(await createLocalWorkspace(FIRST_WORKSPACE_NAME));
	await openWorkspace(next);
};
