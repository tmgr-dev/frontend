import { ref } from 'vue';
import store from '@/store';
import { getCategories } from '@/actions/tmgr/categories';
import { getTasks } from '@/actions/tmgr/tasks';
import { pickTaskByKey, taskKeyPrefixes } from '@/utils/taskKeys';

const prefixes = ref<string[]>([]);
let loadedFor: number | null = null;

/**
 * The key prefixes of the current workspace, loaded once and shared by every
 * rendered message. Categories come from the request cache, so a whole comment
 * list costs at most one extra request. Prefixes differ per workspace, so the
 * load is keyed by the workspace the user is looking at rather than reset from
 * the store — the store must not import this module (it would close the
 * store -> actions -> axios -> store cycle).
 */
export const useTaskKeyPrefixes = () => {
	const workspaceId = store.getters.currentWorkspaceId ?? null;
	if (workspaceId !== loadedFor) {
		loadedFor = workspaceId;
		prefixes.value = [];
		getCategories()
			.then((categories) => {
				if (loadedFor === workspaceId) {
					prefixes.value = taskKeyPrefixes(categories as never[]);
				}
			})
			.catch(() => {
				if (loadedFor === workspaceId) {
					prefixes.value = [];
				}
			});
	}
	return prefixes;
};

/**
 * Open the task a key names. The key is not an id, so it is resolved through
 * the same search the user would type; a key nobody owns simply does nothing.
 */
export const openTaskByKey = async (key: string): Promise<boolean> => {
	try {
		const response = await getTasks({ page: 1, per_page: 20, params: { search: key } });
		const task = pickTaskByKey(response?.data || [], key);
		if (!task) {
			return false;
		}
		store.commit('setCurrentTaskIdForModal', task.id);
		return true;
	} catch {
		return false;
	}
};
