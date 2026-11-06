import * as api from '@/actions/tmgr/personas';
import { toast } from '@/components/ui/toast';
import { useDeletedSkills } from '../deletedSkills';

jest.mock('@/actions/tmgr/personas', () => ({
	listDeletedSharedSkills: jest.fn(),
	getDeletedSharedSkillVersion: jest.fn(),
	restoreDeletedSharedSkill: jest.fn(),
}));
jest.mock('@/components/ui/toast', () => ({ toast: jest.fn() }));

const version = (n: number) => ({
	version: n,
	title: 'Triage',
	kind: 'edit',
	restored_from: null,
	author_name: 'Ann',
	created_at: '2026-01-01T00:00:00Z',
});
const ITEM = {
	slug: 'triage',
	title: 'Triage',
	deleted_at: '2026-11-01T00:00:00Z',
	purge_at: '2026-12-01T00:00:00Z',
	latest_version: 3,
	versions: [version(3), version(2)],
};
const snapshot = (n: number) => ({
	...version(n),
	when: 'w',
	actions: [],
	body: `body ${n}`,
});
const httpError = (status: number, message?: string) => ({
	response: { status, data: { message } },
});

beforeEach(() => {
	jest.resetAllMocks();
	(api.listDeletedSharedSkills as jest.Mock).mockResolvedValue([ITEM]);
	(api.getDeletedSharedSkillVersion as jest.Mock).mockImplementation(
		async (_s: string, n: number) => snapshot(n),
	);
});

describe('useDeletedSkills', () => {
	it('loads the list', async () => {
		const d = useDeletedSkills(jest.fn());
		await d.load();
		expect(d.items.value).toEqual([ITEM]);
		expect(d.error.value).toBe(false);
	});

	it('flags a load failure and retries', async () => {
		(api.listDeletedSharedSkills as jest.Mock).mockRejectedValueOnce(
			new Error('x'),
		);
		const d = useDeletedSkills(jest.fn());
		await d.load();
		expect(d.error.value).toBe(true);
		await d.load();
		expect(d.error.value).toBe(false);
		expect(d.items.value).toHaveLength(1);
	});

	it('expands an entry and collapses it again', async () => {
		const d = useDeletedSkills(jest.fn());
		await d.load();
		d.toggle('triage');
		expect(d.expanded.value).toBe('triage');
		d.toggle('triage');
		expect(d.expanded.value).toBeNull();
	});

	it('previews a version', async () => {
		const d = useDeletedSkills(jest.fn());
		await d.load();
		d.toggle('triage');
		await d.selectVersion(2);
		expect(api.getDeletedSharedSkillVersion).toHaveBeenCalledWith('triage', 2);
		expect(d.selected.value).toBe(2);
		expect(d.preview.value?.body).toBe('body 2');
	});

	it('flags a preview failure', async () => {
		(api.getDeletedSharedSkillVersion as jest.Mock).mockRejectedValue(
			httpError(404),
		);
		const d = useDeletedSkills(jest.fn());
		await d.load();
		d.toggle('triage');
		await d.selectVersion(2);
		expect(d.previewError.value).toBe(true);
		expect(d.preview.value).toBeNull();
	});

	it('restores, toasts, reloads and notifies', async () => {
		(api.restoreDeletedSharedSkill as jest.Mock).mockResolvedValue({});
		const onChanged = jest.fn();
		const d = useDeletedSkills(onChanged);
		await d.load();
		d.toggle('triage');
		await d.selectVersion(2);
		(api.listDeletedSharedSkills as jest.Mock).mockResolvedValue([]);
		await d.restore();
		expect(api.restoreDeletedSharedSkill).toHaveBeenCalledWith('triage', 2);
		expect(toast).toHaveBeenCalledWith({ title: 'Triage restored' });
		expect(d.items.value).toEqual([]);
		expect(d.expanded.value).toBeNull();
		expect(onChanged).toHaveBeenCalled();
	});

	it.each([
		[
			409,
			undefined,
			'A skill with this slug already exists. Rename or delete it first, then restore.',
		],
		[404, undefined, 'This skill is gone for good and cannot be restored.'],
		[422, 'Library is full', 'Library is full'],
	])('maps %i to a clear message', async (status, message, expected) => {
		(api.restoreDeletedSharedSkill as jest.Mock).mockRejectedValue(
			httpError(status, message),
		);
		const onChanged = jest.fn();
		const d = useDeletedSkills(onChanged);
		await d.load();
		d.toggle('triage');
		await d.selectVersion(2);
		await d.restore();
		expect(toast).toHaveBeenCalledWith({
			title: expected,
			variant: 'destructive',
		});
		expect(onChanged).not.toHaveBeenCalled();
		expect(d.restoring.value).toBe(false);
	});

	it('falls back to a generic message on other errors', async () => {
		(api.restoreDeletedSharedSkill as jest.Mock).mockRejectedValue(
			new Error('boom'),
		);
		const d = useDeletedSkills(jest.fn());
		await d.load();
		d.toggle('triage');
		await d.selectVersion(2);
		await d.restore();
		expect(toast).toHaveBeenCalledWith({
			title: 'Could not restore this skill',
			variant: 'destructive',
		});
	});
});
