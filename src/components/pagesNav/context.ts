import type { PageSummary, PageType } from '@/actions/tmgr/pages';
import type { InjectionKey, Ref } from 'vue';

export type PagesTreeSurface = 'sidebar' | 'page';

export interface PagesTreeContext {
	workspaceCode: Ref<string>;
	surface: PagesTreeSurface;
	expanded: Ref<Set<number>>;
	dragging: Ref<number | null>;
	onDragStart: (event: { item: HTMLElement }) => void;
	toggle: (id: number) => void;
	createChild: (parent: PageSummary, type: PageType) => void;
	requestRename: (page: PageSummary) => void;
	requestDelete: (page: PageSummary) => void;
	togglePin: (page: PageSummary) => void;
	exportSubtree: (page: PageSummary) => void;
	importInto: (page: PageSummary, files?: File[]) => void;
	onChange: (
		parentId: number | null,
		event: {
			added?: { element: PageSummary; newIndex: number };
			moved?: { element: PageSummary; newIndex: number };
		},
	) => void;
	renaming: Ref<PageSummary | null>;
	renameTitle: Ref<string>;
	deleting: Ref<PageSummary | null>;
	confirmRename: () => Promise<void>;
	confirmDelete: () => Promise<void>;
}

export const PAGES_TREE_KEY: InjectionKey<PagesTreeContext> =
	Symbol('pagesTree');
