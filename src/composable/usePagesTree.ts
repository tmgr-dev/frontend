import {
	getPagesTree,
	movePage,
	type PageSummary,
} from '@/actions/tmgr/pages';
import { usePagesRealtime } from '@/composable/usePagesRealtime';
import {
	ancestorIds,
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

	const persist = () => {
		if (workspaceId.value !== null)
			saveExpanded(workspaceId.value, expanded.value);
	};

	const expand = (id: number) => {
		if (expanded.value.has(id)) return;
		expanded.value = new Set(expanded.value).add(id);
		persist();
	};

	const toggle = (id: number) => {
		const next = new Set(expanded.value);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		expanded.value = next;
		persist();
	};

	const revealPage = (slug: string | undefined) => {
		const page = pages.value.find((p) => p.slug === slug);
		if (page) ancestorIds(pages.value, page.id).forEach(expand);
	};

	const move = async (id: number, parentId: number | null, position: number) => {
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

	return { pages, tree, loaded, failed, expanded, load, refresh, toggle, expand, revealPage, move };
}
