import {
	deleteSharedSkillFile,
	getSharedSkillFile,
	listSharedSkillFiles,
	type SharedSkillFile,
} from '@/actions/tmgr/personas';
import { toast } from '@/components/ui/toast';
import { ref } from 'vue';

export const STALE_FILES_MESSAGE = 'Files changed elsewhere, reloaded';

export const useSkillFiles = (
	slug: () => string,
	onChanged: () => void = () => {},
	confirm: (message: string) => boolean = (m) => window.confirm(m),
) => {
	const files = ref<SharedSkillFile[]>([]);
	const filesSha256 = ref('');
	const loading = ref(false);
	const openPath = ref<string | null>(null);
	const content = ref('');
	const contentLoading = ref(false);

	let viewRequest = 0;
	let openSha256 = '';

	const clearSelection = () => {
		viewRequest++;
		openPath.value = null;
		openSha256 = '';
		content.value = '';
		contentLoading.value = false;
	};

	const load = async (): Promise<boolean> => {
		loading.value = true;
		try {
			const index = await listSharedSkillFiles(slug());
			files.value = index.files;
			filesSha256.value = index.files_sha256;
			if (openPath.value) {
				const open = files.value.find((f) => f.path === openPath.value);
				if (!open || (openSha256 && open.sha256 !== openSha256)) {
					clearSelection();
				}
			}
			return true;
		} catch {
			toast({ title: 'Could not load files', variant: 'destructive' });
			return false;
		} finally {
			loading.value = false;
		}
	};

	const view = async (path: string) => {
		const request = ++viewRequest;
		openPath.value = path;
		openSha256 = '';
		content.value = '';
		contentLoading.value = true;
		try {
			const file = await getSharedSkillFile(slug(), path);
			if (request !== viewRequest) return;
			content.value = file.content;
			openSha256 = file.sha256;
		} catch {
			if (request !== viewRequest) return;
			openPath.value = null;
			toast({ title: 'Could not load the file', variant: 'destructive' });
		} finally {
			if (request === viewRequest) contentLoading.value = false;
		}
	};

	const remove = async (path: string) => {
		if (!confirm(`Delete ${path}? This cannot be undone.`)) return;
		try {
			await deleteSharedSkillFile(slug(), path, filesSha256.value);
			toast({ title: 'File deleted' });
			await load();
			onChanged();
		} catch (error) {
			const status = (error as { response?: { status?: number } })?.response
				?.status;
			if (status === 409) {
				if (await load()) {
					toast({ title: STALE_FILES_MESSAGE, variant: 'destructive' });
				} else {
					files.value = [];
					filesSha256.value = '';
					clearSelection();
				}
				onChanged();
			} else {
				toast({ title: 'Could not delete the file', variant: 'destructive' });
			}
		}
	};

	return {
		files,
		filesSha256,
		loading,
		openPath,
		content,
		contentLoading,
		load,
		view,
		remove,
	};
};
