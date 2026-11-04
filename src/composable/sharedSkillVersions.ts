import {
	getSharedSkillVersion,
	listSharedSkillVersions,
	pinSharedSkill,
	restoreSharedSkill,
	unpinSharedSkill,
	type SharedSkillVersionRow,
} from '@/actions/tmgr/personas';
import { toast } from '@/components/ui/toast';
import { lineDiff, type DiffLine } from '@/utils/lineDiff';
import { ref } from 'vue';

export const pickDiffPair = (
	versions: SharedSkillVersionRow[],
	selected: number,
	compareWithCurrent: boolean,
): { from: number | null; to: number } => {
	const current = versions.find((v) => v.current);
	if (compareWithCurrent && current && current.version !== selected) {
		return { from: selected, to: current.version };
	}
	const index = versions.findIndex((v) => v.version === selected);
	return { from: versions[index + 1]?.version ?? null, to: selected };
};

export const changePin = (personaUuid: string, slug: string, choice: string) =>
	choice === 'latest'
		? unpinSharedSkill(personaUuid, slug)
		: pinSharedSkill(personaUuid, slug, Number(choice));

export const useSharedSkillHistory = (slug: string, onChanged: () => void) => {
	const versions = ref<SharedSkillVersionRow[]>([]);
	const selected = ref<number | null>(null);
	const compareWithCurrent = ref(false);
	const diff = ref<DiffLine[]>([]);
	const loading = ref(false);
	const restoring = ref(false);

	const bodyOf = async (version: number | null) =>
		version === null ? '' : (await getSharedSkillVersion(slug, version)).body;

	const refreshDiff = async () => {
		if (selected.value === null) {
			diff.value = [];
			return;
		}
		const target = selected.value;
		const { from, to } = pickDiffPair(
			versions.value,
			target,
			compareWithCurrent.value,
		);
		const [before, after] = await Promise.all([bodyOf(from), bodyOf(to)]);
		if (selected.value === target) diff.value = lineDiff(before, after);
	};

	const select = async (version: number) => {
		selected.value = version;
		try {
			await refreshDiff();
		} catch {
			toast({ title: 'Could not load this version', variant: 'destructive' });
		}
	};

	const setCompareWithCurrent = async (value: boolean) => {
		compareWithCurrent.value = value;
		try {
			await refreshDiff();
		} catch {
			toast({ title: 'Could not load this version', variant: 'destructive' });
		}
	};

	const load = async () => {
		loading.value = true;
		try {
			versions.value = await listSharedSkillVersions(slug, false);
			const keep = versions.value.find((v) => v.version === selected.value);
			const next = keep ?? versions.value.find((v) => v.current);
			selected.value = next?.version ?? null;
			await refreshDiff();
		} catch {
			toast({ title: 'Could not load the history', variant: 'destructive' });
		} finally {
			loading.value = false;
		}
	};

	const restore = async (version: number) => {
		const current = versions.value.find((v) => v.current);
		if (!current) return;
		restoring.value = true;
		try {
			await restoreSharedSkill(slug, version, current.version);
			toast({ title: `Version ${version} restored` });
			await load();
			onChanged();
		} catch (error) {
			if (
				(error as { response?: { status?: number } })?.response?.status === 409
			) {
				toast({
					title: 'This skill changed meanwhile. Reload the history.',
					variant: 'destructive',
				});
				await load();
			} else {
				toast({
					title: 'Could not restore this version',
					variant: 'destructive',
				});
			}
		} finally {
			restoring.value = false;
		}
	};

	return {
		versions,
		selected,
		compareWithCurrent,
		diff,
		loading,
		restoring,
		load,
		select,
		setCompareWithCurrent,
		restore,
	};
};
