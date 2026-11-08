import {
	getSharedSkillVersion,
	listSharedSkillVersions,
	pinSharedSkill,
	restoreSharedSkill,
	unpinSharedSkill,
	type SharedSkillVersion,
	type SharedSkillVersionRow,
} from '@/actions/tmgr/personas';
import { toast } from '@/components/ui/toast';
import { t } from '@/i18n';
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

export const metadataChanges = (
	before: Pick<SharedSkillVersion, 'title' | 'when' | 'actions'> | null,
	after: Pick<SharedSkillVersion, 'title' | 'when' | 'actions'>,
): string[] => {
	if (!before) return [];
	const changes: string[] = [];
	if (before.title !== after.title) {
		changes.push(`Title changed: ${before.title} → ${after.title}`);
	}
	if (before.when !== after.when) changes.push('When changed');
	if (JSON.stringify(before.actions) !== JSON.stringify(after.actions)) {
		changes.push('Actions changed');
	}
	return changes;
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
	const metaChanges = ref<string[]>([]);
	let refreshToken = 0;
	const error = ref(false);
	const loading = ref(false);
	const restoring = ref(false);

	const snapshotOf = async (version: number | null) =>
		version === null ? null : getSharedSkillVersion(slug, version);

	const refreshDiff = async () => {
		const token = ++refreshToken;
		diff.value = [];
		metaChanges.value = [];
		error.value = false;
		if (selected.value === null) return;
		const { from, to } = pickDiffPair(
			versions.value,
			selected.value,
			compareWithCurrent.value,
		);
		try {
			const [before, after] = await Promise.all([
				snapshotOf(from),
				snapshotOf(to),
			]);
			if (token !== refreshToken || !after) return;
			diff.value = lineDiff(before?.body ?? '', after.body);
			metaChanges.value = metadataChanges(before, after);
		} catch {
			if (token === refreshToken) error.value = true;
		}
	};

	const select = async (version: number) => {
		selected.value = version;
		await refreshDiff();
	};

	const setCompareWithCurrent = async (value: boolean) => {
		compareWithCurrent.value = value;
		await refreshDiff();
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
			toast({
				title: t('settingsPages.sharedSkillHistory.loadFailed'),
				variant: 'destructive',
			});
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
			toast({
				title: t('settingsPages.sharedSkillHistory.restored', { version }),
			});
			await load();
			onChanged();
		} catch (error) {
			if (
				(error as { response?: { status?: number } })?.response?.status === 409
			) {
				toast({
					title: t('settingsPages.sharedSkillHistory.changedMeanwhile'),
					variant: 'destructive',
				});
				await load();
				onChanged();
			} else {
				toast({
					title: t('settingsPages.sharedSkillHistory.restoreFailed'),
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
		metaChanges,
		error,
		loading,
		restoring,
		load,
		select,
		setCompareWithCurrent,
		restore,
	};
};
