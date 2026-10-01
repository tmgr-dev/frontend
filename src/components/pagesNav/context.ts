import type { PageSummary, PageType } from '@/actions/tmgr/pages';
import type { InjectionKey, Ref } from 'vue';

export interface PagesTreeContext {
	workspaceCode: Ref<string>;
	expanded: Ref<Set<number>>;
	toggle: (id: number) => void;
	createChild: (parent: PageSummary, type: PageType) => void;
	requestRename: (page: PageSummary) => void;
	requestDelete: (page: PageSummary) => void;
	togglePin: (page: PageSummary) => void;
	onChange: (
		parentId: number | null,
		event: {
			added?: { element: PageSummary; newIndex: number };
			moved?: { element: PageSummary; newIndex: number };
		},
	) => void;
}

export const PAGES_TREE_KEY: InjectionKey<PagesTreeContext> =
	Symbol('pagesTree');
