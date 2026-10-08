import { usePagesImportFlow } from '../usePagesImportFlow';

const mockApi = {
	readImportFiles: jest.fn(),
	planImport: jest.fn(),
	runImport: jest.fn(),
};

jest.mock('@/utils/pages/markdownIo/api', () => mockApi);

const page = (key: string, parentKey: string | null, conflict = false) => ({
	key,
	path: null,
	parentKey,
	title: key,
	type: 'plain',
	properties: null,
	body: '',
	sourceId: null,
	sourceWorkspace: null,
	conflict,
});

const target = () => ({ parentId: 7, workspaceCode: 'ws' });
const file = new File(['# a'], 'a.md');

const planOf = (pages: ReturnType<typeof page>[]) => ({
	pages,
	files: {},
	warnings: [{ path: 'x.md', message: 'plan warning' }],
});

describe('usePagesImportFlow', () => {
	beforeEach(() => {
		jest.resetAllMocks();
		mockApi.readImportFiles.mockResolvedValue({
			files: [],
			warnings: [{ path: null, message: 'read warning' }],
		});
	});

	it('builds an indented preview and merges warnings', async () => {
		mockApi.planImport.mockResolvedValue(
			planOf([page('a', null), page('b', 'a'), page('c', 'b')]),
		);
		const flow = usePagesImportFlow(target);
		await flow.start([file]);
		expect(mockApi.planImport).toHaveBeenCalledWith([], target());
		expect(flow.step.value).toBe('preview');
		expect(flow.rows.value.map((row) => row.depth)).toEqual([0, 1, 2]);
		expect(flow.warnings.value).toHaveLength(2);
		expect(flow.hasConflicts.value).toBe(false);
		expect(flow.importCount.value).toBe(3);
	});

	it('counts conflicts and skips their subtree under the skip policy', async () => {
		mockApi.planImport.mockResolvedValue(
			planOf([page('a', null, true), page('b', 'a'), page('c', null)]),
		);
		const flow = usePagesImportFlow(target);
		await flow.start([file]);
		expect(flow.hasConflicts.value).toBe(true);
		expect(flow.policy.value).toBe('rename');
		expect(flow.importCount.value).toBe(3);
		flow.policy.value = 'skip';
		expect(flow.importCount.value).toBe(1);
		expect(flow.skippedCount.value).toBe(2);
	});

	it('runs the import with the chosen policy and reports the result', async () => {
		mockApi.planImport.mockResolvedValue(
			planOf([page('a', null, true), page('b', null)]),
		);
		mockApi.runImport.mockImplementation(async (_plan, options) => {
			options.onProgress({ done: 1, total: 2, current: 'a' });
			return { created: [], skipped: [], warnings: [], error: null };
		});
		const flow = usePagesImportFlow(target);
		await flow.start([file]);
		flow.policy.value = 'import';
		await flow.submit();
		expect(mockApi.runImport).toHaveBeenCalledWith(
			expect.objectContaining({ pages: expect.any(Array) }),
			expect.objectContaining({
				parentId: 7,
				workspaceCode: 'ws',
				policy: 'import',
			}),
		);
		expect(flow.progress.value).toEqual({ done: 1, total: 2, current: 'a' });
		expect(flow.step.value).toBe('done');
	});

	it('uses rename when there are no conflicts', async () => {
		mockApi.planImport.mockResolvedValue(planOf([page('a', null)]));
		mockApi.runImport.mockResolvedValue({
			created: [],
			skipped: [],
			warnings: [],
			error: null,
		});
		const flow = usePagesImportFlow(target);
		await flow.start([file]);
		flow.policy.value = 'skip';
		await flow.submit();
		expect(mockApi.runImport.mock.calls[0][1].policy).toBe('rename');
	});

	it('shows read and plan errors inline and returns to the picker', async () => {
		mockApi.readImportFiles.mockRejectedValue(new Error('Too many entries'));
		const flow = usePagesImportFlow(target);
		await flow.start([file]);
		expect(flow.step.value).toBe('pick');
		expect(flow.error.value).toBe('Too many entries');
	});

	it('rejects an empty plan', async () => {
		mockApi.planImport.mockResolvedValue(planOf([]));
		const flow = usePagesImportFlow(target);
		await flow.start([file]);
		expect(flow.step.value).toBe('pick');
		expect(flow.error.value).toMatch(/No Markdown pages/);
	});

	it('turns a thrown run error into a result with the error', async () => {
		mockApi.planImport.mockResolvedValue(planOf([page('a', null)]));
		mockApi.runImport.mockRejectedValue(new Error('boom'));
		const flow = usePagesImportFlow(target);
		await flow.start([file]);
		await flow.submit();
		expect(flow.step.value).toBe('done');
		expect(flow.result.value?.error).toBe('boom');
	});
});
