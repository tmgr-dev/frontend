import { compileTemplate, parse } from '@vue/compiler-sfc';
import { readFileSync } from 'fs';
import { join } from 'path';
import { createSSRApp, defineComponent, h } from 'vue';
import { renderToString } from 'vue/server-renderer';

const mockList = jest.fn();
const mockGet = jest.fn();
const mockDelete = jest.fn();
const mockToast = jest.fn();

jest.mock('@/actions/tmgr/personas', () => ({
	listSharedSkillFiles: (...a: unknown[]) => mockList(...a),
	getSharedSkillFile: (...a: unknown[]) => mockGet(...a),
	deleteSharedSkillFile: (...a: unknown[]) => mockDelete(...a),
}));
jest.mock('@/components/ui/toast', () => ({
	toast: (...a: unknown[]) => mockToast(...a),
}));

import { STALE_FILES_MESSAGE, useSkillFiles } from '@/composable/skillFiles';

const index = (paths: string[], sha = 'sha-1') => ({
	files: paths.map((path) => ({
		path,
		size: 12,
		sha256: 'x',
		updated_at: '2026-11-07',
	})),
	files_sha256: sha,
});

beforeEach(() => jest.resetAllMocks());

describe('useSkillFiles', () => {
	it('lists files', async () => {
		mockList.mockResolvedValue(index(['a.md', 'scripts/run.sh']));
		const state = useSkillFiles(() => 'demo');
		await state.load();
		expect(mockList).toHaveBeenCalledWith('demo');
		expect(state.files.value.map((f) => f.path)).toEqual([
			'a.md',
			'scripts/run.sh',
		]);
		expect(state.filesSha256.value).toBe('sha-1');
	});

	it('loads the content of a viewed file', async () => {
		mockGet.mockResolvedValue({
			path: 'a.md',
			content: '<script>alert(1)</script>',
			size: 25,
			sha256: 'x',
		});
		const state = useSkillFiles(() => 'demo');
		await state.view('a.md');
		expect(mockGet).toHaveBeenCalledWith('demo', 'a.md');
		expect(state.content.value).toBe('<script>alert(1)</script>');
	});

	it('deletes with the expected hash after confirm', async () => {
		mockList
			.mockResolvedValueOnce(index(['a.md', 'b.md'], 'old'))
			.mockResolvedValueOnce(index(['b.md'], 'new'));
		mockDelete.mockResolvedValue(undefined);
		const changed = jest.fn();
		const state = useSkillFiles(
			() => 'demo',
			changed,
			() => true,
		);
		await state.load();
		await state.remove('a.md');
		expect(mockDelete).toHaveBeenCalledWith('demo', 'a.md', 'old');
		expect(state.files.value.map((f) => f.path)).toEqual(['b.md']);
		expect(changed).toHaveBeenCalled();
	});

	it('does nothing when the confirm is declined', async () => {
		const state = useSkillFiles(
			() => 'demo',
			jest.fn(),
			() => false,
		);
		await state.remove('a.md');
		expect(mockDelete).not.toHaveBeenCalled();
	});

	it('reloads and warns on 409', async () => {
		mockList
			.mockResolvedValueOnce(index(['a.md'], 'old'))
			.mockResolvedValueOnce(index(['a.md', 'c.md'], 'fresh'));
		mockDelete.mockRejectedValue({ response: { status: 409 } });
		const state = useSkillFiles(
			() => 'demo',
			jest.fn(),
			() => true,
		);
		await state.load();
		await state.remove('a.md');
		expect(mockList).toHaveBeenCalledTimes(2);
		expect(state.filesSha256.value).toBe('fresh');
		expect(mockToast).toHaveBeenCalledWith(
			expect.objectContaining({ title: STALE_FILES_MESSAGE }),
		);
		expect(STALE_FILES_MESSAGE).toBe('Files changed elsewhere, reloaded');
	});

	it('ignores a late failure for an earlier file', async () => {
		let rejectA: (e: unknown) => void = () => {};
		mockGet
			.mockImplementationOnce(
				() => new Promise((_, reject) => (rejectA = reject)),
			)
			.mockResolvedValueOnce({
				path: 'b.md',
				content: 'B',
				size: 1,
				sha256: 'sb',
			});
		const state = useSkillFiles(() => 'demo');
		const a = state.view('a.md');
		await state.view('b.md');
		rejectA(new Error('late'));
		await a;
		expect(state.openPath.value).toBe('b.md');
		expect(state.content.value).toBe('B');
		expect(state.contentLoading.value).toBe(false);
		expect(mockToast).not.toHaveBeenCalled();
	});

	it('ignores a late finish for an earlier file', async () => {
		let resolveA: (v: unknown) => void = () => {};
		mockGet
			.mockImplementationOnce(
				() => new Promise((resolve) => (resolveA = resolve)),
			)
			.mockImplementationOnce(() => new Promise(() => {}));
		const state = useSkillFiles(() => 'demo');
		const a = state.view('a.md');
		void state.view('b.md');
		resolveA({ path: 'a.md', content: 'A', size: 1, sha256: 'sa' });
		await a;
		expect(state.openPath.value).toBe('b.md');
		expect(state.content.value).toBe('');
		expect(state.contentLoading.value).toBe(true);
	});

	it('does not claim a reload after a 409 when the reload fails', async () => {
		mockList
			.mockResolvedValueOnce(index(['a.md'], 'old'))
			.mockRejectedValueOnce(new Error('down'));
		mockDelete.mockRejectedValue({ response: { status: 409 } });
		const state = useSkillFiles(
			() => 'demo',
			jest.fn(),
			() => true,
		);
		await state.load();
		await state.remove('a.md');
		expect(mockToast).not.toHaveBeenCalledWith(
			expect.objectContaining({ title: STALE_FILES_MESSAGE }),
		);
		expect(mockToast).toHaveBeenCalledWith(
			expect.objectContaining({ title: 'Could not load files' }),
		);
		expect(state.files.value).toEqual([]);
		expect(state.filesSha256.value).toBe('');
	});

	it('load reports success', async () => {
		mockList.mockResolvedValueOnce(index(['a.md'])).mockRejectedValueOnce(1);
		const state = useSkillFiles(() => 'demo');
		expect(await state.load()).toBe(true);
		expect(await state.load()).toBe(false);
	});

	it('clears the content when the selected file changed or is gone', async () => {
		const withSha = (path: string, sha256: string) => ({
			files: [{ path, size: 1, sha256, updated_at: 'x' }],
			files_sha256: sha256,
		});
		mockGet.mockResolvedValue({
			path: 'a.md',
			content: 'old',
			size: 3,
			sha256: 's1',
		});
		mockList
			.mockResolvedValueOnce(withSha('a.md', 's1'))
			.mockResolvedValueOnce(withSha('a.md', 's1'))
			.mockResolvedValueOnce(withSha('a.md', 's2'))
			.mockResolvedValueOnce(withSha('b.md', 's3'));
		const state = useSkillFiles(() => 'demo');
		await state.load();
		await state.view('a.md');
		await state.load();
		expect(state.content.value).toBe('old');
		await state.load();
		expect(state.openPath.value).toBeNull();
		expect(state.content.value).toBe('');
		await state.view('a.md');
		await state.load();
		expect(state.openPath.value).toBeNull();
	});

	it('shows a generic error on other failures', async () => {
		mockDelete.mockRejectedValue({ response: { status: 500 } });
		const state = useSkillFiles(
			() => 'demo',
			jest.fn(),
			() => true,
		);
		await state.remove('a.md');
		expect(mockList).not.toHaveBeenCalled();
		expect(mockToast).toHaveBeenCalledWith(
			expect.objectContaining({ title: 'Could not delete the file' }),
		);
	});
});

const sfc = (name: string) => readFileSync(join(__dirname, '..', name), 'utf8');

const renderFiles = async (state: Record<string, unknown>) => {
	const { descriptor } = parse(sfc('SkillFiles.vue'));
	const { code } = compileTemplate({
		source: descriptor.template!.content,
		filename: 'SkillFiles.vue',
		id: 'skill-files',
		compilerOptions: { mode: 'function' },
	});
	const render = new Function('Vue', code)(require('vue'));
	const Button = defineComponent({
		setup:
			(_p, { slots }) =>
			() =>
				h('button', slots.default?.()),
	});
	const Comp = defineComponent({
		components: { Button },
		setup: () => state,
		render,
	});
	return renderToString(createSSRApp(Comp));
};

describe('SkillFiles template', () => {
	const base = {
		slug: 'demo',
		loading: false,
		files: [{ path: 'a.md', size: 12 }],
		openPath: null,
		content: '',
		contentLoading: false,
		view: () => {},
		remove: () => {},
		formatBytes: (n: number) => `${n} B`,
	};

	it('lists path and size', async () => {
		const html = await renderFiles(base);
		expect(html).toContain('a.md');
		expect(html).toContain('12 B');
	});

	it('renders file content as text, never as markup', async () => {
		const html = await renderFiles({
			...base,
			openPath: 'a.md',
			content: '<script>alert(1)</script>',
		});
		expect(html).toContain('&lt;script&gt;alert(1)&lt;/script&gt;');
		expect(html).not.toContain('<script>');
		expect(html).toMatch(/<pre[^>]*data-testid="skill-file-content"/);
	});

	it('never uses v-html', () => {
		expect(sfc('SkillFiles.vue')).not.toContain('v-html');
	});
});

describe('library wiring', () => {
	const flat = (name: string) => sfc(name).replace(/\s+/g, ' ');

	it('hides the Files entry when the skill has no files or the API lacks files_count', () => {
		expect(flat('SkillsLibrary.vue')).toContain('v-if="skill.files_count"');
		expect(flat('SkillsLibrary.vue')).toContain(
			'return skill?.files_count ? skill : null',
		);
	});

	it('says files are not pinned next to the pin label', () => {
		const source = flat('PersonaSharedSkills.vue');
		expect(source).toContain('skill.pinned_version && skill.files_count');
		expect(source).toContain('Files are not pinned');
	});

	it('shows the file count of deleted skills only when present', () => {
		expect(flat('DeletedSkills.vue')).toContain('v-if="item.files_count"');
	});
});
