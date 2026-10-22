<template>
	<PageContainer>
		<PageHeader
			title="Archive"
			:back="`/${workspaceCode}/pages`"
			back-label="Pages"
		>
			<template v-if="loaded && !failed && pages.length" #actions>
				<button
					type="button"
					class="inline-flex h-9 items-center gap-2 rounded-md px-3 text-sm text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-900/20"
					data-testid="pages-archive-empty"
					@click="openEmpty"
				>
					<Trash2 class="h-4 w-4" />
					Delete all
				</button>
			</template>
		</PageHeader>
		<p v-if="!loaded" class="text-sm text-ink-subtle">Loading…</p>
		<p v-else-if="failed" class="text-sm text-destructive">
			Failed to load archive
		</p>
		<p v-else-if="!pages.length" class="text-sm text-ink-subtle">
			Archive is empty.
		</p>
		<ul
			v-else
			class="divide-y divide-line overflow-hidden rounded-card border border-line bg-surface"
		>
			<li
				v-for="page in pages"
				:key="page.id"
				class="flex flex-wrap items-center gap-x-3 gap-y-2 px-4 py-3"
				:data-page-id="page.id"
			>
				<Archive class="h-4 w-4 shrink-0 text-ink-subtle" />
				<div class="min-w-0 flex-1 basis-48">
					<p class="truncate text-sm font-medium text-ink">{{ page.title }}</p>
					<p class="text-xs text-ink-subtle">
						{{ `Archived ${formatDeleted(page.deleted_at)}` }}
					</p>
				</div>
				<div class="ml-auto flex shrink-0 gap-2 whitespace-nowrap">
					<button
						type="button"
						class="inline-flex h-8 items-center gap-1.5 rounded-md border border-gray-300 px-3 text-sm text-ink hover:bg-surface-hover disabled:opacity-50 dark:border-gray-600"
						:disabled="restoring === page.id"
						@click="restore(page)"
					>
						<Undo2 class="h-4 w-4" />
						Restore
					</button>
					<button
						type="button"
						class="inline-flex h-8 items-center gap-1.5 rounded-md border border-red-300 px-3 text-sm text-red-600 hover:bg-red-50 disabled:opacity-50 dark:border-red-800 dark:text-red-400 dark:hover:bg-red-900/20"
						:disabled="busy"
						@click="purging = page"
					>
						<Trash2 class="h-4 w-4" />
						Delete permanently
					</button>
				</div>
			</li>
		</ul>

		<AlertDialog
			:open="!!purging"
			@update:open="(open) => !open && !busy && (purging = null)"
		>
			<AlertDialogContent>
				<AlertDialogHeader>
					<AlertDialogTitle>Delete permanently?</AlertDialogTitle>
					<AlertDialogDescription class="[overflow-wrap:anywhere]">
						"{{ purging?.title }}" and its archived subpages will be deleted
						forever. This can't be undone.
					</AlertDialogDescription>
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel :disabled="busy">Cancel</AlertDialogCancel>
					<Button variant="destructive" :disabled="busy" @click="confirmPurge">
						Delete permanently
					</Button>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>

		<AlertDialog
			:open="emptyOpen"
			@update:open="(open) => !open && !busy && (emptyOpen = false)"
		>
			<AlertDialogContent>
				<form class="grid gap-4" @submit.prevent="confirmEmpty">
					<AlertDialogHeader>
						<AlertDialogTitle>Delete all archived pages?</AlertDialogTitle>
						<AlertDialogDescription>
							{{ countLabel }} will be deleted forever. This can't be undone.
						</AlertDialogDescription>
					</AlertDialogHeader>
					<label class="grid gap-1.5 text-sm text-ink">
						<span>
							Type <span class="font-semibold">{{ CONFIRM_WORD }}</span> to
							confirm
						</span>
						<input
							v-model="confirmWord"
							data-selectable
							autocomplete="off"
							class="h-9 rounded-md border border-input bg-background px-3 text-sm outline-none dark:border-input dark:bg-background"
						/>
					</label>
					<AlertDialogFooter>
						<AlertDialogCancel type="button" :disabled="busy">
							Cancel
						</AlertDialogCancel>
						<Button
							type="submit"
							variant="destructive"
							:disabled="busy || !confirmed"
						>
							Delete all
						</Button>
					</AlertDialogFooter>
				</form>
			</AlertDialogContent>
		</AlertDialog>
	</PageContainer>
</template>

<script lang="ts">
	import {
		emptyPagesTrash,
		getPagesTrash,
		permanentlyDeletePage,
		restorePage,
		type PageSummary,
	} from '@/actions/tmgr/pages';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import {
		AlertDialog,
		AlertDialogCancel,
		AlertDialogContent,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
	} from '@/components/ui/alert-dialog';
	import { Button } from '@/components/ui/button';
	import { useToast } from '@/components/ui/toast';
	import { pageUrl } from '@/utils/pagesTree';
	import { Archive, Trash2, Undo2 } from 'lucide-vue-next';
	import { computed, defineComponent, onMounted, ref, watch } from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	const CONFIRM_WORD = 'delete';

	const plural = (count: number, word: string) =>
		`${count} ${word}${count === 1 ? '' : 's'}`;

	export default defineComponent({
		name: 'PagesTrash',
		components: {
			AlertDialog,
			AlertDialogCancel,
			AlertDialogContent,
			AlertDialogDescription,
			AlertDialogFooter,
			AlertDialogHeader,
			AlertDialogTitle,
			Archive,
			Button,
			PageContainer,
			PageHeader,
			Trash2,
			Undo2,
		},
		setup() {
			const route = useRoute();
			const router = useRouter();
			const store = useStore();
			const toaster = useToast();
			const workspaceCode = computed(() => String(route.params.workspace_code));
			const pages = ref<PageSummary[]>([]);
			const loaded = ref(false);
			const failed = ref(false);
			const restoring = ref<number | null>(null);
			const purging = ref<PageSummary | null>(null);
			const emptyOpen = ref(false);
			const confirmWord = ref('');
			const busy = ref(false);

			const load = async () => {
				try {
					pages.value = await getPagesTrash();
					failed.value = false;
				} catch {
					failed.value = true;
				} finally {
					loaded.value = true;
				}
			};

			onMounted(load);
			watch(() => store.state.pagesEvent?.seq, load);

			const restore = async (page: PageSummary) => {
				restoring.value = page.id;
				try {
					const restored = await restorePage(page.id);
					router.push(pageUrl(workspaceCode.value, restored.slug));
				} catch {
					toaster.toast({
						title: 'Failed to restore page',
						variant: 'destructive',
					});
				} finally {
					restoring.value = null;
				}
			};

			const fail = (title: string) =>
				toaster.toast({ title, variant: 'destructive' });

			const confirmPurge = async () => {
				const page = purging.value;
				if (!page || busy.value) return;
				busy.value = true;
				try {
					await permanentlyDeletePage(page.id);
					toaster.toast({ title: 'Page deleted permanently' });
				} catch (error: any) {
					const status = error?.response?.status;
					if (status === 403)
						fail("You don't have permission to delete this page");
					else if (status === 404)
						toaster.toast({ title: 'This page was already deleted' });
					else if (status === 422)
						toaster.toast({ title: 'This page is no longer archived' });
					else fail('Failed to delete page');
					await load();
				} finally {
					busy.value = false;
					purging.value = null;
				}
			};

			const openEmpty = () => {
				confirmWord.value = '';
				emptyOpen.value = true;
			};

			const confirmed = computed(
				() => confirmWord.value.trim().toLowerCase() === CONFIRM_WORD,
			);

			const confirmEmpty = async () => {
				if (!confirmed.value || busy.value) return;
				busy.value = true;
				try {
					const { skipped } = await emptyPagesTrash();
					if (skipped > 0) {
						toaster.toast({
							title: 'Some pages were kept',
							description: `${plural(
								skipped,
								'archived page',
							)} you can't delete stayed in the archive.`,
						});
					} else {
						toaster.toast({ title: 'Archive emptied' });
					}
				} catch (error: any) {
					if (error?.response?.status === 403)
						fail("You don't have permission to empty the archive");
					else fail('Failed to empty the archive');
					await load();
				} finally {
					busy.value = false;
					emptyOpen.value = false;
				}
			};

			const countLabel = computed(() =>
				pages.value.length === 1
					? 'The archived page and its subpages'
					: `All ${pages.value.length} archived pages and their subpages`,
			);

			const formatDeleted = (value?: string | null) =>
				value ? new Date(value).toLocaleString() : '';

			return {
				CONFIRM_WORD,
				workspaceCode,
				pages,
				loaded,
				failed,
				restoring,
				restore,
				purging,
				confirmPurge,
				emptyOpen,
				openEmpty,
				confirmWord,
				confirmed,
				confirmEmpty,
				countLabel,
				busy,
				formatDeleted,
			};
		},
	});
</script>
