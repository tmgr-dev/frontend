import { getCategories, type Category } from '@/actions/tmgr/categories';
import store from '@/store';
import {
	buildCategoryGroups,
	flattenGroups,
	type CategoryNode,
} from '@/utils/categorySettingsPage';
import { requestCache } from '@/utils/requestCache';
import { computed, ref } from 'vue';

export type TreeCategory = Category & { cursor_connected?: boolean };

const categories = ref<TreeCategory[]>([]);
const loaded = ref(false);
const pending = ref(false);
const error = ref<string | null>(null);
const forbidden = ref(false);
let loadedFor: unknown;
let request = 0;

const workspaceKey = () => store.getters.currentWorkspaceId ?? null;

const groups = computed(() => buildCategoryGroups(categories.value));
const nodes = computed<CategoryNode<TreeCategory>[]>(() =>
	flattenGroups(groups.value),
);

async function refresh(): Promise<void> {
	const id = ++request;
	const key = workspaceKey();
	if (loadedFor !== key) {
		loaded.value = false;
		categories.value = [];
		forbidden.value = false;
	}
	loadedFor = key;
	pending.value = true;
	error.value = null;
	try {
		requestCache.invalidate('categories');
		const data = await getCategories();
		if (id !== request) return;
		categories.value = (Array.isArray(data) ? data : []) as TreeCategory[];
		loaded.value = true;
	} catch (e: any) {
		if (id !== request) return;
		if (e?.response?.status === 403) forbidden.value = true;
		error.value = e?.response?.data?.message || 'Could not load categories.';
	} finally {
		if (id === request) pending.value = false;
	}
}

function ensure(): void {
	const key = workspaceKey();
	if (key === null) return;
	if (loadedFor === key && (loaded.value || pending.value)) return;
	void refresh();
}

export function useCategoryTree() {
	return {
		categories,
		groups,
		nodes,
		loaded,
		pending,
		error,
		forbidden,
		refresh,
		ensure,
	};
}

export const lastCategoryKey = (workspaceCode: string) =>
	`tmgr:lastCategory:${workspaceCode}`;
