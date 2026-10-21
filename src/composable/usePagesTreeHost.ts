import type { PageSummary, PageType } from '@/actions/tmgr/pages';
import {
	PAGES_TREE_KEY,
	type PagesTreeContext,
	type PagesTreeSurface,
} from '@/components/pagesNav/context';
import { usePagesActions } from '@/composable/usePagesActions';
import { usePagesMarkdownIo } from '@/composable/usePagesMarkdownIo';
import { usePagesTree } from '@/composable/usePagesTree';
import { descendantIds } from '@/utils/pagesTree';
import { computed, nextTick, provide, ref, type Ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { useStore } from 'vuex';

export function usePagesTreeHost(
	workspaceId: Ref<number | null>,
	workspaceCode: Ref<string>,
	surface: PagesTreeSurface,
) {
	const route = useRoute();
	const router = useRouter();
	const store = useStore();
	const modulesKnown = computed(
		() => store.getters['featureToggles/modulesKnown'] === true,
	);
	const actions = usePagesActions(() => workspaceCode.value);
	const io = usePagesMarkdownIo(() => workspaceCode.value);
	const tree = usePagesTree(workspaceId, modulesKnown, () =>
		actions.fail('Failed to move page'),
	);

	const renaming = ref<PageSummary | null>(null);
	const renameTitle = ref('');
	const deleting = ref<PageSummary | null>(null);
	const dragging = ref<number | null>(null);

	const createChild = async (parent: PageSummary, type: PageType) => {
		tree.expand(parent.id);
		await actions.create(type, parent.id);
	};

	const requestRename = (page: PageSummary) => {
		renaming.value = page;
		renameTitle.value = page.title;
	};

	const confirmRename = async () => {
		const page = renaming.value;
		if (!page) return;
		renaming.value = null;
		if (await actions.rename(page, renameTitle.value)) await tree.refresh();
	};

	const confirmDelete = async () => {
		const page = deleting.value;
		deleting.value = null;
		if (!page) return;
		const affected = descendantIds(tree.pages.value, page.id);
		const viewing = tree.pages.value.find((p) => p.slug === route.params.slug);
		if (await actions.remove(page)) {
			if (viewing && affected.has(viewing.id)) {
				router.push(`/${workspaceCode.value}/pages`);
			}
			await tree.refresh();
		}
	};

	const togglePin = async (page: PageSummary) => {
		if (!(await actions.togglePin(page))) return;
		await tree.refresh();
		await nextTick();
		// The row may be re-created in another list (pinned ↔ recent), which drops focus to the body.
		if (document.activeElement && document.activeElement !== document.body)
			return;
		document
			.querySelector<HTMLElement>(
				`[data-pages-tree="${surface}"] [data-page-id="${page.id}"] [aria-label="Page actions"]`,
			)
			?.focus();
	};

	const onChange: PagesTreeContext['onChange'] = (parentId, event) => {
		const change = event.added ?? event.moved;
		if (change) tree.move(change.element.id, parentId, change.newIndex);
	};

	const onDragStart = (event: { item: HTMLElement }) => {
		const id = event.item
			.querySelector('[data-page-id]')
			?.getAttribute('data-page-id');
		dragging.value = id ? Number(id) : null;
	};

	provide(PAGES_TREE_KEY, {
		workspaceCode,
		surface,
		dragging,
		onDragStart,
		expanded: tree.expanded,
		toggle: tree.toggle,
		createChild,
		requestRename,
		requestDelete: (page) => (deleting.value = page),
		togglePin,
		onChange,
		exportSubtree: (page) => void io.exportSubtree(page.id),
		importInto: (page, files) => io.openImport(page, files),
		renaming,
		renameTitle,
		deleting,
		confirmRename,
		confirmDelete,
	});

	return { tree, actions, io, dragging, onDragStart, onChange };
}
