import { invoke } from '@tauri-apps/api/core';

import router from '@/router';
import store from '@/store';
import { requestCache } from '@/utils/requestCache';

import { deleteDemo } from '../demoApi';
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
