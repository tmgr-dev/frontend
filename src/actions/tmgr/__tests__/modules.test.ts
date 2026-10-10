import $axios from '@/plugins/axios';
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

	it('does not call the server for a local workspace', async () => {
		expect(await getWorkspaceModules(-1)).toBeNull();
		expect(http.get).not.toHaveBeenCalled();
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
