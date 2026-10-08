const mockStore = {
	getters: {
		currentWorkspaceId: 5 as number | string | null,
		currentWorkspace: { code: 'acme' },
	},
	commit: jest.fn(),
};
const mockAxios = { get: jest.fn(), post: jest.fn(), patch: jest.fn() };
const mockFiles = {
	presignUpload: jest.fn(),
	putToStorage: jest.fn(),
	attachFileToPage: jest.fn(),
};

jest.mock('@/store', () => ({ __esModule: true, default: mockStore }));
jest.mock('@/plugins/axios', () => ({ __esModule: true, default: mockAxios }));
jest.mock('@/actions/tmgr/files', () => mockFiles);

import {
	exportSubtree,
	exportWorkspace,
	planImport,
	readImportFiles,
	runImport,
} from '../api';
import { buildImportPlan } from '../importPlan';
import { file } from './fakeApi';

const reply = (data: unknown) => ({ data: { data } });
const options = {
	parentId: null,
	policy: 'rename' as const,
	workspaceCode: 'acme',
};

const requests = () =>
	[mockAxios.get, mockAxios.post, mockAxios.patch].flatMap(
		(fn) => fn.mock.calls,
	);

beforeEach(() => {
	jest.clearAllMocks();
	mockStore.getters.currentWorkspaceId = 5;
	mockAxios.get.mockImplementation(async (url: string) =>
		reply(url === 'pages/tree' ? [] : { id: 1, version: 1, files: [] }),
	);
	mockAxios.post.mockImplementation(async () =>
		reply({ id: 1, slug: 'a', title: 'A', version: 1 }),
	);
	mockAxios.patch.mockImplementation(async () => reply({ id: 1, version: 2 }));
	mockFiles.presignUpload.mockResolvedValue({ key: 'k' });
	mockFiles.attachFileToPage.mockResolvedValue({ id: 77 });
});

describe('workspace pinning', () => {
	it('sends the workspace id on every page and file request of an import', async () => {
		const plan = buildImportPlan(
			[
				file('A.md', '![p](p.png) [t](tmgr://task/1)'),
				file('p.png', new Uint8Array([1])),
			],
			{ existingTitles: [] },
		);
		const result = await runImport(plan, options);
		expect(result.error).toBeNull();
		expect(requests().length).toBeGreaterThan(2);
		for (const [, ...rest] of requests()) {
			const config = rest[rest.length - 1];
			expect(config.params).toEqual({ workspace_id: 5 });
			expect(config.headers).toEqual({ 'X-Workspace-Id': '5' });
		}
		expect(mockFiles.presignUpload).toHaveBeenCalledWith(expect.any(File), 5);
		expect(mockFiles.attachFileToPage).toHaveBeenCalledWith(
			1,
			expect.any(File),
			expect.anything(),
			5,
		);
	});

	it('pins a local workspace without a header', async () => {
		mockStore.getters.currentWorkspaceId = -3;
		const plan = buildImportPlan([file('A.md', 'a')], { existingTitles: [] });
		await runImport(plan, options);
		const config = mockAxios.post.mock.calls[0][2];
		expect(config.params).toEqual({ workspace_id: -3 });
		expect(config.headers).toBeUndefined();
	});

	it('stops an import when the workspace changes mid-run', async () => {
		const plan = buildImportPlan(
			[file('A.md', '[t](tmgr://task/1)'), file('B.md', 'b')],
			{ existingTitles: [] },
		);
		mockAxios.post.mockImplementation(async () => {
			mockStore.getters.currentWorkspaceId = 6;
			return reply({ id: 1, slug: 'a', title: 'A', version: 1 });
		});
		const result = await runImport(plan, options);
		expect(result.error).toBe('The workspace changed during import');
		expect(mockAxios.post).toHaveBeenCalledTimes(1);
		expect(mockAxios.patch).not.toHaveBeenCalled();
		expect(result.incomplete).toEqual(['A']);
	});

	it('refuses to run a plan made in another workspace', async () => {
		const plan = await planImport([file('A.md', 'a')], {
			parentId: null,
			workspaceCode: 'acme',
		});
		expect(plan.workspaceId).toBe(5);
		mockStore.getters.currentWorkspaceId = 6;
		const result = await runImport(plan, options);
		expect(result.error).toBe('The workspace changed during import');
		expect(mockAxios.post).not.toHaveBeenCalled();
	});

	it('aborts an export when the workspace changes', async () => {
		mockAxios.get.mockImplementation(async (url: string) => {
			if (url === 'pages/tree') {
				return reply([
					{
						id: 1,
						title: 'A',
						slug: 'a',
						type: 'plain',
						parent_id: null,
						position: 0,
					},
				]);
			}
			mockStore.getters.currentWorkspaceId = 6;
			return reply({
				id: 1,
				slug: 'a',
				title: 'A',
				body: '',
				version: 1,
				files: [],
			});
		});
		await expect(exportSubtree(1)).rejects.toThrow(
			'The workspace changed during export',
		);
		mockStore.getters.currentWorkspaceId = 5;
		await expect(exportWorkspace('acme')).rejects.toThrow(
			'The workspace changed during export',
		);
		for (const [, config] of mockAxios.get.mock.calls) {
			expect(config.params.workspace_id).toBeDefined();
		}
	});
});

describe('readImportFiles', () => {
	it('rejects an oversized selection before reading', async () => {
		const big = {
			name: 'a.md',
			size: 60 * 1024 * 1024,
			arrayBuffer: jest.fn(),
		};
		await expect(readImportFiles([big as any])).rejects.toThrow(/larger than/);
		expect(big.arrayBuffer).not.toHaveBeenCalled();
	});
});
