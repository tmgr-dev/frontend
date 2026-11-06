import {
	getDeletedSharedSkillVersion,
	listDeletedSharedSkills,
	restoreDeletedSharedSkill,
	type DeletedSharedSkill,
	type DeletedSharedSkillVersion,
} from '@/actions/tmgr/personas';
import { toast } from '@/components/ui/toast';
import { computed, ref } from 'vue';

const restoreErrorMessage = (error: unknown): string => {
	const response = (
		error as { response?: { status?: number; data?: { message?: string } } }
	)?.response;
	if (response?.status === 409) {
		return 'A skill with this slug already exists. Rename or delete it first, then restore.';
	}
	if (response?.status === 404) {
		return 'This skill is gone for good and cannot be restored.';
	}
	if (response?.status === 422 && response.data?.message) {
		return response.data.message;
	}
	return 'Could not restore this skill';
};

export const useDeletedSkills = (onChanged: () => void) => {
	const items = ref<DeletedSharedSkill[]>([]);
	const loading = ref(false);
	const error = ref(false);
	const expanded = ref<string | null>(null);
	const selected = ref<number | null>(null);
	const preview = ref<DeletedSharedSkillVersion | null>(null);
	const previewError = ref(false);
	const restoring = ref(false);
	let previewToken = 0;

	const current = computed(
		() => items.value.find((i) => i.slug === expanded.value) ?? null,
	);

	const resetSelection = () => {
		previewToken++;
		selected.value = null;
		preview.value = null;
		previewError.value = false;
	};

	const load = async () => {
		loading.value = true;
		error.value = false;
		try {
			items.value = await listDeletedSharedSkills(false);
			if (expanded.value && !current.value) {
				expanded.value = null;
				resetSelection();
			}
		} catch {
			error.value = true;
		} finally {
			loading.value = false;
		}
	};

	const toggle = (slug: string) => {
		resetSelection();
		expanded.value = expanded.value === slug ? null : slug;
	};

	const selectVersion = async (version: number) => {
		const slug = expanded.value;
		if (!slug) return;
		const token = ++previewToken;
		selected.value = version;
		preview.value = null;
		previewError.value = false;
		try {
			const result = await getDeletedSharedSkillVersion(slug, version);
			if (token === previewToken) preview.value = result;
		} catch {
			if (token === previewToken) previewError.value = true;
		}
	};

	const restore = async () => {
		const skill = current.value;
		if (!skill || selected.value === null) return;
		restoring.value = true;
		try {
			await restoreDeletedSharedSkill(skill.slug, selected.value);
			toast({ title: `${skill.title} restored` });
			expanded.value = null;
			resetSelection();
			await load();
			onChanged();
		} catch (e) {
			toast({ title: restoreErrorMessage(e), variant: 'destructive' });
		} finally {
			restoring.value = false;
		}
	};

	return {
		items,
		loading,
		error,
		expanded,
		current,
		selected,
		preview,
		previewError,
		restoring,
		load,
		toggle,
		selectVersion,
		restore,
	};
};
