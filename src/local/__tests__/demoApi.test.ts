import { invoke } from '@tauri-apps/api/core';

import router from '@/router';
import store from '@/store';
import { requestCache } from '@/utils/requestCache';

import { registerGhostStop } from '@/onboarding/ghost';
import { ghostStage } from '@/onboarding/ghostStage';
import { toursEngine } from '@/tours/useTours';
import { completeDemoEntry, deleteDemo } from '../demoApi';
import * as runtime from '../runtime';
import type { LocalWorkspace } from '../types';

jest.mock('@tauri-apps/api/core', () => ({
	invoke: jest.fn(),
}));

jest.mock('@/store', () => ({
	__esModule: true,
	default: { dispatch: jest.fn() },
}));

jest.mock('@/router', () => ({
	__esModule: true,
	default: { push: jest.fn() },
}));

jest.mock('@/utils/requestCache', () => ({
	requestCache: { invalidate: jest.fn() },
}));

jest.mock('@/tours/useTours', () => ({
	toursEngine: jest.fn(),
}));

jest.mock('../runtime', () => ({
	...jest.requireActual('../runtime'),
	listLocalWorkspaces: jest.fn(),
	activeLocalWorkspaceCode: jest.fn(),
	forgetLocalWorkspace: jest.fn(),
	setActiveLocalWorkspace: jest.fn(),
	createLocalWorkspace: jest.fn(),
	localContext: jest.fn(),
}));

const DEMO: LocalWorkspace = {
	id: -7,
	name: 'Demo: Launch week',
	code: 'demo',
	schema_version: 12,
	created_at: '',
	path: '/tmp/demo',
	database: '/tmp/demo/workspace.db',
	demo: true,
};

const REAL: LocalWorkspace = {
	id: -1,
	name: 'My workspace',
	code: 'personal',
	schema_version: 12,
	created_at: '',
	path: '/tmp/personal',
	database: '/tmp/personal/workspace.db',
};

describe('deleteDemo', () => {
	beforeEach(() => {
		jest.clearAllMocks();
	});

	it('refuses a workspace that is not the demo and deletes nothing', async () => {
		jest.mocked(runtime.listLocalWorkspaces).mockResolvedValue([REAL, DEMO]);
		await expect(deleteDemo('personal')).rejects.toThrow(
			'Only the demo workspace can be deleted this way',
		);
		expect(invoke).not.toHaveBeenCalled();
		expect(runtime.setActiveLocalWorkspace).not.toHaveBeenCalled();
	});

	it('deletes the active demo and opens the first real workspace', async () => {
		jest
			.mocked(runtime.listLocalWorkspaces)
			.mockResolvedValueOnce([DEMO, REAL])
			.mockResolvedValueOnce([REAL]);
		jest.mocked(runtime.activeLocalWorkspaceCode).mockReturnValue('demo');

		await deleteDemo('demo');

		expect(invoke).toHaveBeenCalledWith('local_workspace_delete_demo', {
			code: 'demo',
		});
		expect(runtime.forgetLocalWorkspace).toHaveBeenCalledWith('demo');
		expect(runtime.setActiveLocalWorkspace).toHaveBeenCalledWith(REAL);
		expect(runtime.createLocalWorkspace).not.toHaveBeenCalled();
		expect(router.push).toHaveBeenCalledWith('/local-personal/board');
		expect(requestCache.invalidate).toHaveBeenCalledWith('workspaces');
		expect(store.dispatch).toHaveBeenCalledWith('loadWorkspaces');
	});

	it('waits for the ghost to stop before the folder goes to the Trash', async () => {
		jest
			.mocked(runtime.listLocalWorkspaces)
			.mockResolvedValueOnce([DEMO, REAL])
			.mockResolvedValueOnce([REAL]);
		jest.mocked(runtime.activeLocalWorkspaceCode).mockReturnValue('demo');
		const order: string[] = [];
		jest.mocked(invoke).mockImplementation(async () => {
			order.push('invoke');
		});
		const off = registerGhostStop(async () => {
			await Promise.resolve();
			order.push('stopped');
		});

		await deleteDemo('demo');
		off();

		expect(order).toEqual(['stopped', 'invoke']);
	});

	it('creates My workspace when no real workspace is left', async () => {
		jest
			.mocked(runtime.listLocalWorkspaces)
			.mockResolvedValueOnce([DEMO])
			.mockResolvedValueOnce([]);
		jest.mocked(runtime.activeLocalWorkspaceCode).mockReturnValue('demo');
		jest.mocked(runtime.createLocalWorkspace).mockResolvedValue(REAL);

		await deleteDemo('demo');

		expect(runtime.createLocalWorkspace).toHaveBeenCalledWith('My workspace');
		expect(runtime.setActiveLocalWorkspace).toHaveBeenCalledWith(REAL);
		expect(router.push).toHaveBeenCalledWith('/local-personal/board');
	});
});

describe('completeDemoEntry', () => {
	const engine = {
		api: {
			setIntent: jest.fn(async () => {}),
			startSequence: jest.fn(async () => {}),
		},
	};

	beforeEach(() => {
		jest.clearAllMocks();
		jest.mocked(toursEngine).mockReturnValue(engine as any);
		ghostStage.value = 'wheel';
	});

	it('seeds the starter tasks into the real workspace, finishes the stage and opens the board', async () => {
		jest.mocked(runtime.listLocalWorkspaces).mockResolvedValue([DEMO, REAL]);
		const order: string[] = [];
		const off = registerGhostStop(async () => void order.push('stopped'));
		engine.api.setIntent.mockImplementationOnce(async () => {
			order.push('seeded');
		});

		await completeDemoEntry();
		off();

		expect(order).toEqual(['stopped', 'seeded']);
		expect(runtime.setActiveLocalWorkspace).toHaveBeenCalledWith(REAL);
		expect(engine.api.setIntent).toHaveBeenCalledWith('human', {
			seed: true,
			workspace: 'personal',
		});
		expect(ghostStage.value).toBe('finished');
		expect(router.push).toHaveBeenCalledWith('/local-personal/board');
		expect(engine.api.startSequence).toHaveBeenCalledWith(
			['desktop.local-workspace'],
			'first_run',
		);
		expect(invoke).not.toHaveBeenCalled();
	});

	it('keeps the stage when seeding fails so the panel can be retried', async () => {
		jest.mocked(runtime.listLocalWorkspaces).mockResolvedValue([DEMO, REAL]);
		engine.api.setIntent.mockRejectedValueOnce(new Error('disk full'));

		await expect(completeDemoEntry()).rejects.toThrow('disk full');

		expect(ghostStage.value).toBe('wheel');
		expect(router.push).not.toHaveBeenCalled();
	});

	it('can also send the demo to the Trash', async () => {
		jest.mocked(runtime.listLocalWorkspaces).mockResolvedValue([DEMO, REAL]);
		jest.mocked(runtime.activeLocalWorkspaceCode).mockReturnValue('personal');

		await completeDemoEntry({ deleteDemo: true });

		expect(invoke).toHaveBeenCalledWith('local_workspace_delete_demo', {
			code: 'demo',
		});
		expect(router.push).toHaveBeenLastCalledWith('/local-personal/board');
	});
});
