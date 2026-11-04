import * as api from '@/actions/tmgr/personas';
import { toast } from '@/components/ui/toast';
import {
	changePin,
	metadataChanges,
	pickDiffPair,
	useSharedSkillHistory,
} from '../sharedSkillVersions';

jest.mock('@/actions/tmgr/personas', () => ({
	listSharedSkillVersions: jest.fn(),
	getSharedSkillVersion: jest.fn(),
	restoreSharedSkill: jest.fn(),
	pinSharedSkill: jest.fn(),
	unpinSharedSkill: jest.fn(),
}));
jest.mock('@/components/ui/toast', () => ({ toast: jest.fn() }));

const row = (version: number, extra = {}) => ({
	version,
	kind: 'edit',
	restored_from: null,
	title: 'Triage',
	author: { id: 1, name: 'Ann' },
	created_at: '2026-01-01T00:00:00Z',
	current: false,
	pinned_personas: [],
	...extra,
});
const VERSIONS = [row(5, { current: true }), row(3), row(1)];
const snapshot = (version: number) => ({
	...row(version),
	when: 'w',
	actions: [],
	body: `body ${version}`,
});

beforeEach(() => {
	jest.resetAllMocks();
	(api.listSharedSkillVersions as jest.Mock).mockResolvedValue(VERSIONS);
	(api.getSharedSkillVersion as jest.Mock).mockImplementation(
		async (_s: string, v: number) => snapshot(v),
	);
});

describe('pickDiffPair', () => {
	it('diffs a version against the previous kept one', () => {
		expect(pickDiffPair(VERSIONS as any, 3, false)).toEqual({
			from: 1,
			to: 3,
		});
	});

	it('diffs the oldest version against nothing', () => {
		expect(pickDiffPair(VERSIONS as any, 1, false)).toEqual({
			from: null,
			to: 1,
		});
	});

	it('diffs against current when comparing with current', () => {
		expect(pickDiffPair(VERSIONS as any, 3, true)).toEqual({
			from: 3,
			to: 5,
		});
	});

	it('falls back to the previous version when selecting current', () => {
		expect(pickDiffPair(VERSIONS as any, 5, true)).toEqual({
			from: 3,
			to: 5,
		});
	});
});

describe('useSharedSkillHistory', () => {
	it('loads versions and selects the current one', async () => {
		const h = useSharedSkillHistory('triage', jest.fn());
		await h.load();
		expect(api.listSharedSkillVersions).toHaveBeenCalledWith('triage', false);
		expect(h.selected.value).toBe(5);
		expect(h.diff.value).toEqual([
			{ type: 'del', text: 'body 3' },
			{ type: 'add', text: 'body 5' },
		]);
	});

	it('diffs a selected version against the previous one', async () => {
		const h = useSharedSkillHistory('triage', jest.fn());
		await h.load();
		await h.select(3);
		expect(api.getSharedSkillVersion).toHaveBeenCalledWith('triage', 1);
		expect(h.diff.value).toEqual([
			{ type: 'del', text: 'body 1' },
			{ type: 'add', text: 'body 3' },
		]);
	});

	it('diffs the oldest version against empty', async () => {
		const h = useSharedSkillHistory('triage', jest.fn());
		await h.load();
		await h.select(1);
		expect(h.diff.value).toEqual([{ type: 'add', text: 'body 1' }]);
	});

	it('compares with current when toggled', async () => {
		const h = useSharedSkillHistory('triage', jest.fn());
		await h.load();
		await h.select(3);
		await h.setCompareWithCurrent(true);
		expect(h.diff.value).toEqual([
			{ type: 'del', text: 'body 3' },
			{ type: 'add', text: 'body 5' },
		]);
	});

	it('restores with expected_version = current and notifies', async () => {
		(api.restoreSharedSkill as jest.Mock).mockResolvedValue({ version: 6 });
		const changed = jest.fn();
		const h = useSharedSkillHistory('triage', changed);
		await h.load();
		await h.restore(3);
		expect(api.restoreSharedSkill).toHaveBeenCalledWith('triage', 3, 5);
		expect(changed).toHaveBeenCalled();
		expect(api.listSharedSkillVersions).toHaveBeenCalledTimes(2);
	});

	it('toasts and reloads on 409 without notifying', async () => {
		(api.restoreSharedSkill as jest.Mock).mockRejectedValue({
			response: { status: 409 },
		});
		const changed = jest.fn();
		const h = useSharedSkillHistory('triage', changed);
		await h.load();
		await h.restore(3);
		expect(toast).toHaveBeenCalledWith(
			expect.objectContaining({
				title: 'This skill changed meanwhile. Reload the history.',
				variant: 'destructive',
			}),
		);
		expect(changed).not.toHaveBeenCalled();
		expect(api.listSharedSkillVersions).toHaveBeenCalledTimes(2);
	});

	it('toasts a generic error on other failures', async () => {
		(api.restoreSharedSkill as jest.Mock).mockRejectedValue(new Error('x'));
		const h = useSharedSkillHistory('triage', jest.fn());
		await h.load();
		await h.restore(3);
		expect(toast).toHaveBeenCalledWith(
			expect.objectContaining({ title: 'Could not restore this version' }),
		);
	});
});

describe('overlapping refreshes', () => {
	it('keeps the result of the latest refresh when an older one resolves last', async () => {
		const h = useSharedSkillHistory('triage', jest.fn());
		await h.load();
		await h.select(3);
		let releaseFirst: () => void = () => {};
		const gate = new Promise<void>((r) => (releaseFirst = r));
		(api.getSharedSkillVersion as jest.Mock).mockImplementation(
			async (_s: string, v: number) => {
				if (v === 1) await gate;
				return snapshot(v);
			},
		);
		const first = h.setCompareWithCurrent(true);
		(api.getSharedSkillVersion as jest.Mock).mockImplementation(
			async (_s: string, v: number) => snapshot(v),
		);
		await h.setCompareWithCurrent(false);
		releaseFirst();
		await first;
		expect(h.compareWithCurrent.value).toBe(false);
		expect(h.diff.value).toEqual([
			{ type: 'del', text: 'body 1' },
			{ type: 'add', text: 'body 3' },
		]);
	});
});

describe('metadataChanges', () => {
	const base = { title: 'A', when: 'w', actions: [{ a: 1 }] };

	it('is empty without a previous version or changes', () => {
		expect(metadataChanges(null, base)).toEqual([]);
		expect(metadataChanges(base, { ...base })).toEqual([]);
	});

	it('lists title, when and actions changes', () => {
		expect(
			metadataChanges(base, { title: 'B', when: 'x', actions: [] }),
		).toEqual(['Title changed: A → B', 'When changed', 'Actions changed']);
	});
});

describe('changePin', () => {
	it('pins to a numeric version', async () => {
		await changePin('uuid-1', 'triage', '3');
		expect(api.pinSharedSkill).toHaveBeenCalledWith('uuid-1', 'triage', 3);
		expect(api.unpinSharedSkill).not.toHaveBeenCalled();
	});

	it('unpins on latest', async () => {
		await changePin('uuid-1', 'triage', 'latest');
		expect(api.unpinSharedSkill).toHaveBeenCalledWith('uuid-1', 'triage');
		expect(api.pinSharedSkill).not.toHaveBeenCalled();
	});
});
