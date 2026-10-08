import type {
	ConflictPolicy,
	ImportPlan,
	ImportProgress,
	ImportResult,
	IoWarning,
	PlannedPage,
} from '@/utils/pages/markdownIo/types';
import { computed, ref } from 'vue';

export type ImportStep = 'pick' | 'reading' | 'preview' | 'running' | 'done';

export interface PlannedRow {
	page: PlannedPage;
	depth: number;
	skipped: boolean;
}

export function usePagesImportFlow(target: () => {
	parentId: number | null;
	workspaceCode: string;
}) {
	const step = ref<ImportStep>('pick');
	const error = ref<string | null>(null);
	const plan = ref<ImportPlan | null>(null);
	const warnings = ref<IoWarning[]>([]);
	const policy = ref<ConflictPolicy>('rename');
	const progress = ref<ImportProgress>({ done: 0, total: 0, current: '' });
	const result = ref<ImportResult | null>(null);

	const hasConflicts = computed(
		() => !!plan.value?.pages.some((page) => page.conflict),
	);

	const rows = computed<PlannedRow[]>(() => {
		const pages = plan.value?.pages ?? [];
		const byKey = new Map(pages.map((page) => [page.key, page]));
		const skippedKeys = new Set<string>();
		return pages.map((page) => {
			const skipped =
				policy.value === 'skip' &&
				hasConflicts.value &&
				(page.conflict ||
					(page.parentKey !== null && skippedKeys.has(page.parentKey)));
			if (skipped) skippedKeys.add(page.key);
			let depth = 0;
			for (
				let parent = page.parentKey ? byKey.get(page.parentKey) : undefined;
				parent && depth < 64;
				parent = parent.parentKey ? byKey.get(parent.parentKey) : undefined
			) {
				depth += 1;
			}
			return { page, depth, skipped };
		});
	});

	const conflictCount = computed(
		() => rows.value.filter((row) => row.page.conflict).length,
	);
	const skippedCount = computed(
		() => rows.value.filter((row) => row.skipped).length,
	);
	const importCount = computed(() => rows.value.length - skippedCount.value);

	const messageOf = (cause: unknown) =>
		cause instanceof Error && cause.message ? cause.message : 'Import failed';

	const start = async (files: File[]) => {
		error.value = null;
		step.value = 'reading';
		try {
			const api = await import('@/utils/pages/markdownIo/api');
			const read = await api.readImportFiles(files);
			const planned = await api.planImport(read.files, target());
			if (!planned.pages.length) {
				error.value = 'No Markdown pages found in the selected files.';
				step.value = 'pick';
				return;
			}
			plan.value = planned;
			warnings.value = [...read.warnings, ...planned.warnings];
			policy.value = 'rename';
			step.value = 'preview';
		} catch (cause) {
			error.value = messageOf(cause);
			step.value = 'pick';
		}
	};

	const submit = async () => {
		if (!plan.value || step.value !== 'preview') return;
		const { parentId, workspaceCode } = target();
		progress.value = { done: 0, total: importCount.value, current: '' };
		step.value = 'running';
		try {
			const api = await import('@/utils/pages/markdownIo/api');
			result.value = await api.runImport(plan.value, {
				parentId,
				workspaceCode,
				policy: hasConflicts.value ? policy.value : 'rename',
				onProgress: (next) => {
					progress.value = next;
				},
			});
		} catch (cause) {
			result.value = {
				created: [],
				skipped: [],
				warnings: [],
				error: messageOf(cause),
			};
		}
		step.value = 'done';
	};

	return {
		step,
		error,
		plan,
		warnings,
		policy,
		progress,
		result,
		hasConflicts,
		rows,
		conflictCount,
		skippedCount,
		importCount,
		start,
		submit,
	};
}
