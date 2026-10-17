import { getWorkspaceStatuses } from '@/actions/tmgr/workspaces';
import $axios from '@/plugins/axios';
import { requestCache } from '@/utils/requestCache';
import {
	getWorkspaceFeatureToggles,
	getWorkspaceModules,
	hideModuleForMe,
	saveModulesChoice,
	setModuleEnabled,
} from '../modules';

jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn(), put: jest.fn() },
}));

jest.mock('@/actions/tmgr/workspaces', () => ({
	getWorkspaceStatuses: jest.fn().mockResolvedValue([]),
}));

const http = $axios as unknown as { get: jest.Mock; put: jest.Mock };

const payload = {
	configured: true,
	can_manage: true,
	enforcement: 'enforce',
	packs: [],
	presets: [],
	modules: [{ key: 'pages', enabled: false }],
};

beforeEach(() => {
	http.get.mockReset();
	http.put.mockReset();
});

describe('getWorkspaceModules', () => {
	it('reads the modules endpoint and unwraps the envelope', async () => {
		http.get.mockResolvedValue({ data: { data: payload } });
		const result = await getWorkspaceModules(5);
		expect(http.get).toHaveBeenCalledWith('/workspaces/5/modules');
		expect(result?.modules).toEqual([{ key: 'pages', enabled: false }]);
		expect(result?.canManage).toBe(true);
	});

	it('returns null on 404', async () => {
		http.get.mockRejectedValue({ response: { status: 404 } });
		expect(await getWorkspaceModules(5)).toBeNull();
	});

	it('asks the modules endpoint for a local workspace too', async () => {
		http.get.mockResolvedValue({ data: { data: payload } });
		const result = await getWorkspaceModules(-1);
		expect(http.get).toHaveBeenCalledWith('/workspaces/-1/modules');
		expect(result?.canManage).toBe(true);
	});

	it('rethrows other errors', async () => {
		http.get.mockRejectedValue({ response: { status: 500 } });
		await expect(getWorkspaceModules(5)).rejects.toBeTruthy();
	});
});

describe('getWorkspaceFeatureToggles', () => {
	it('returns the modules as a key map', async () => {
		http.get.mockResolvedValue({ data: { data: payload } });
		expect(await getWorkspaceFeatureToggles(5)).toEqual({
			pages: { key: 'pages', enabled: false },
		});
	});

	it('falls back to feature-toggles when modules 404', async () => {
		http.get.mockImplementation((url: string) =>
			url.endsWith('/modules')
				? Promise.reject({ response: { status: 404 } })
				: Promise.resolve({ data: { data: { board: { enabled: true } } } }),
		);
		expect(await getWorkspaceFeatureToggles(5)).toEqual({
			board: { enabled: true },
		});
		expect(http.get).toHaveBeenLastCalledWith('/workspaces/5/feature-toggles');
	});
});

describe('writes', () => {
	beforeEach(() => {
		http.put.mockResolvedValue({ data: { data: payload } });
		http.get.mockResolvedValue({ data: { data: payload } });
	});

	it('switches a workspace module through feature-toggles and reloads', async () => {
		const result = await setModuleEnabled(
			5,
			{ key: 'pages', scope: 'workspace' },
			true,
		);
		expect(http.put).toHaveBeenCalledWith('/workspaces/5/feature-toggles', {
			features: { pages: true },
		});
		expect(result?.modules[0].key).toBe('pages');
	});

	it('switches a user-scoped module through the user endpoint', async () => {
		await setModuleEnabled(5, { key: 'mcp', scope: 'user' }, false);
		expect(http.put).toHaveBeenCalledWith('/user/feature-toggles', {
			features: { mcp: false },
		});
	});

	it('hides a module for the caller', async () => {
		const result = await hideModuleForMe(5, 'pages', true);
		expect(http.put).toHaveBeenCalledWith('/workspaces/5/modules/hidden', {
			pages: true,
		});
		expect(result.configured).toBe(true);
	});

	it('saves a preset', async () => {
		await saveModulesChoice(5, { preset: 'developer' });
		expect(http.put).toHaveBeenCalledWith('/workspaces/5/modules', {
			preset: 'developer',
		});
	});

	it('saves an explicit module map', async () => {
		await saveModulesChoice(5, { modules: { pages: true, graph: false } });
		expect(http.put).toHaveBeenCalledWith('/workspaces/5/modules', {
			modules: { pages: true, graph: false },
		});
	});
});

describe('getWorkspaceModules single flight', () => {
	it('sends one request for concurrent reads of the same workspace', async () => {
		http.get.mockResolvedValue({ data: { data: payload } });
		await Promise.all([getWorkspaceModules(7), getWorkspaceModules(7)]);
		expect(http.get).toHaveBeenCalledTimes(1);
	});

	it('does not share a request between workspaces', async () => {
		http.get.mockResolvedValue({ data: { data: payload } });
		await Promise.all([getWorkspaceModules(7), getWorkspaceModules(8)]);
		expect(http.get).toHaveBeenCalledTimes(2);
	});

	it('does not reuse a finished read', async () => {
		http.get.mockResolvedValue({ data: { data: payload } });
		await getWorkspaceModules(7);
		await getWorkspaceModules(7);
		expect(http.get).toHaveBeenCalledTimes(2);
	});

	it('re-reads after a save even when an older read is still in flight', async () => {
		let release: (value: unknown) => void = () => {};
		http.get.mockReturnValueOnce(new Promise((resolve) => (release = resolve)));
		http.get.mockResolvedValueOnce({ data: { data: payload } });
		http.put.mockResolvedValue({ data: {} });
		const stale = getWorkspaceModules(9);
		await setModuleEnabled(9, { key: 'pages', scope: 'workspace' }, true);
		expect(http.get).toHaveBeenCalledTimes(2);
		release({ data: { data: payload } });
		await stale;
	});
});

describe('saveModulesChoice status refresh', () => {
	it('drops the cached statuses and reloads the workspace statuses', async () => {
		const invalidate = jest.spyOn(requestCache, 'invalidate');
		http.put.mockResolvedValue({ data: { data: payload } });
		await saveModulesChoice(5, { modules: { workflow: false } });
		expect(invalidate).toHaveBeenCalledWith('workspace-statuses');
		expect(invalidate).toHaveBeenCalledWith('statuses');
		expect(invalidate).toHaveBeenCalledWith(/^statuses-workspace-/);
		expect(getWorkspaceStatuses).toHaveBeenCalledWith(false);
		invalidate.mockRestore();
	});
});

describe('setModuleEnabled for workflow', () => {
	it('sends workflow through the modules endpoint and returns its notices', async () => {
		http.put.mockResolvedValue({
			data: {
				data: {
					...payload,
					modules: [{ key: 'workflow', enabled: false }],
					notices: ['Added a Done status'],
				},
			},
		});
		const result = await setModuleEnabled(
			5,
			{ key: 'workflow', scope: 'workspace' },
			false,
		);
		expect(http.put).toHaveBeenCalledWith('/workspaces/5/modules', {
			modules: { workflow: false },
		});
		expect(http.get).not.toHaveBeenCalled();
		expect(result?.notices).toEqual(['Added a Done status']);
	});
});
