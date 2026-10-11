import {
	archiveTarget,
	DONE_DISABLED_TOOLTIP,
	doneTarget,
	isBoardVisible,
	isSimpleMode,
	isTaskArchived,
	isTaskDone,
	openTarget,
	toggleDoneTarget,
	type StatusLike,
	type TaskLike,
} from '@/utils/simpleStatus';
import { computed } from 'vue';
import { useStore } from 'vuex';

export function useWorkflowMode() {
	const store = useStore();

	const isOn = (key: string): boolean =>
		store.getters['featureToggles/isFeatureEnabled'](key);

	const simpleMode = computed(() => isSimpleMode(isOn));
	const boardVisible = computed(() => isBoardVisible(isOn));

	const workspaceStatuses = computed<StatusLike[]>(
		() => (store.state.workspaceStatuses as StatusLike[]) || [],
	);

	const openStatusId = computed(() => openTarget(workspaceStatuses.value));
	const doneStatusId = computed(() => doneTarget(workspaceStatuses.value));
	const archiveStatusId = computed(() =>
		archiveTarget(workspaceStatuses.value),
	);
	const canMarkDone = computed(() => doneStatusId.value !== null);
	const doneTooltip = computed(() =>
		canMarkDone.value ? '' : DONE_DISABLED_TOOLTIP,
	);

	const isDoneTask = (task: TaskLike | null | undefined): boolean =>
		isTaskDone(task, workspaceStatuses.value);
	const isArchivedTask = (task: TaskLike | null | undefined): boolean =>
		isTaskArchived(task, workspaceStatuses.value);
	const toggleTarget = (task: TaskLike | null | undefined): number | null =>
		toggleDoneTarget(task, workspaceStatuses.value);

	return {
		simpleMode,
		boardVisible,
		workspaceStatuses,
		openStatusId,
		doneStatusId,
		archiveStatusId,
		canMarkDone,
		doneTooltip,
		isDoneTask,
		isArchivedTask,
		toggleTarget,
	};
}
