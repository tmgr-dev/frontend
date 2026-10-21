import { getPagesTree, movePage, type PageSummary } from '@/actions/tmgr/pages';
import { usePagesRealtime } from '@/composable/usePagesRealtime';
import {
	applyMove,
	buildPagesTree,
	loadExpanded,
	saveExpanded,
} from '@/utils/pagesTree';
import { computed, ref, watch, type Ref } from 'vue';

export function usePagesTree(
	workspaceId: Ref<number | null>,
	enabled: Ref<boolean>,
	onMoveError: () => void,
) {
	const pages = ref<PageSummary[]>([]);
	const loaded = ref(false);
	const failed = ref(false);
	const expanded = ref<Set<number>>(new Set());
	let request = 0;

	const tree = computed(() => buildPagesTree(pages.value));

	const load = async (useCache = true) => {
		if (!enabled.value || workspaceId.value === null) return;
		const current = ++request;
		const forWorkspace = workspaceId.value;
		try {
			const rows = await getPagesTree(useCache);
			if (current !== request || forWorkspace !== workspaceId.value) return;
			pages.value = rows;
			failed.value = false;
		} catch {
			if (current === request) failed.value = true;
		} finally {
			if (current === request) loaded.value = true;
		}
	};

	const refresh = () => load(false);

	watch(
		[workspaceId, enabled],
		([id]) => {
			pages.value = [];
			loaded.value = false;
			expanded.value = id === null ? new Set() : loadExpanded(id);
			load();
		},
		{ immediate: true },
	);

	usePagesRealtime(workspaceId, enabled, refresh);

	// The sidebar and All pages each hold a tree: start from the stored set so one doesn't drop the other's toggles.
	const update = (change: (next: Set<number>) => void) => {
		const id = workspaceId.value;
		const next = new Set(id === null ? expanded.value : loadExpanded(id));
		change(next);
		expanded.value = next;
		if (id !== null) saveExpanded(id, next);
	};

	const expand = (id: number) => update((next) => next.add(id));

	const toggle = (id: number) =>
		update((next) => {
			if (expanded.value.has(id)) next.delete(id);
			else next.add(id);
		});

	const move = async (
		id: number,
		parentId: number | null,
		position: number,
	) => {
		const snapshot = pages.value;
		pages.value = applyMove(pages.value, id, parentId, position);
		if (parentId !== null) expand(parentId);
		try {
			await movePage(id, parentId, position);
		} catch {
			pages.value = snapshot;
			onMoveError();
		}
		await load(false);
	};

	return {
		pages,
		tree,
		loaded,
		failed,
		expanded,
		load,
		refresh,
		toggle,
		expand,
		move,
	};
}
