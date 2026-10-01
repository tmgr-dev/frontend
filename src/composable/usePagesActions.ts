import {
	createPage,
	deletePage,
	getPage,
	pinPage,
	unpinPage,
	updatePage,
	type PageSummary,
	type PageType,
} from '@/actions/tmgr/pages';
import { useToast } from '@/components/ui/toast';
import { DEFAULT_PAGE_TITLE, pageUrl } from '@/utils/pagesTree';
import { useRouter } from 'vue-router';

export function usePagesActions(getWorkspaceCode: () => string | undefined) {
	const router = useRouter();
	const toaster = useToast();

	const fail = (title: string) =>
		toaster.toast({ title, variant: 'destructive' });

	const open = (page: Pick<PageSummary, 'slug'>) => {
		const code = getWorkspaceCode();
		if (code) router.push(pageUrl(code, page.slug));
	};

	const create = async (
		type: PageType = 'plain',
		parentId: number | null = null,
	): Promise<PageSummary | null> => {
		try {
			const page = await createPage({
				title: DEFAULT_PAGE_TITLE,
				type,
				parent_id: parentId,
			});
			open(page);
			return page;
		} catch {
			fail('Failed to create page');
			return null;
		}
	};

	const rename = async (page: PageSummary, title: string): Promise<boolean> => {
		const next = title.trim();
		if (!next || next === page.title) return true;
		try {
			const current = await getPage(page.id);
			await updatePage(page.id, { version: current.version, title: next });
			return true;
		} catch {
			fail('Failed to rename page');
			return false;
		}
	};

	const togglePin = async (page: PageSummary): Promise<boolean> => {
		try {
			await (page.pinned ? unpinPage(page.id) : pinPage(page.id));
			return true;
		} catch {
			fail('Failed to change pin');
			return false;
		}
	};

	const remove = async (page: PageSummary): Promise<boolean> => {
		try {
			await deletePage(page.id);
			return true;
		} catch {
			fail('Failed to delete page');
			return false;
		}
	};

	return { create, rename, togglePin, remove, open, fail };
}
