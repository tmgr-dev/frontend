<template>
	<PageContainer width="wide">
		<div v-if="loading" class="py-16 text-center text-ink-subtle">
			Loading...
		</div>
		<div v-else-if="loadError" role="alert" class="py-16 text-center">
			<p class="text-ink">Failed to load history</p>
			<button
				class="mt-2 text-blue-600 underline dark:text-blue-400"
				@click="load"
			>
				Retry
			</button>
		</div>

		<template v-else-if="page">
			<PageHeader
				:title="`History: ${page.title}`"
				:back="`/${workspaceCode}/pages/${page.slug}`"
				back-label="Back to page"
			/>

			<div class="grid gap-6 lg:grid-cols-[16rem_minmax(0,1fr)]">
				<ul class="space-y-1" data-testid="versions-list">
					<li v-for="version in versions" :key="version.version">
						<button
							type="button"
							class="w-full rounded-md border px-3 py-2 text-left text-sm"
							:class="
								selected === version.version
									? 'border-blue-500 bg-blue-50 dark:border-blue-400 dark:bg-blue-900/20'
									: 'border-gray-200 hover:bg-gray-50 dark:border-gray-700 dark:hover:bg-gray-800'
							"
							:data-testid="`version-${version.version}`"
							@click="select(version.version)"
						>
							<span class="flex items-center justify-between gap-2">
								<span class="font-medium text-gray-900 dark:text-gray-100"
									>v{{ version.version }}</span
								>
								<span
									v-if="version.version === page.version"
									class="rounded bg-green-100 px-1.5 text-2xs text-green-800 dark:bg-green-900/40 dark:text-green-300"
									>current</span
								>
							</span>
							<span class="mt-1 block">
								<AuthorBadge :author="authorOf(version.author)" :size="16" />
							</span>
							<span
								class="mt-1 block text-xs text-gray-500 dark:text-gray-400"
								>{{ formatDate(version.created_at) }}</span
							>
							<span
								v-if="version.summary"
								class="mt-1 block truncate text-xs text-gray-600 dark:text-gray-300"
								>{{ version.summary }}</span
							>
						</button>
					</li>
				</ul>

				<div class="min-w-0">
					<div v-if="!selected" class="text-ink-subtle">Select a version</div>
					<div v-else-if="snapshotLoading" class="text-ink-subtle">
						Loading...
					</div>
					<template v-else-if="snapshot">
						<div class="mb-4 flex flex-wrap items-center justify-between gap-2">
							<div
								class="flex items-center gap-1 rounded-lg bg-gray-100 p-1 dark:bg-gray-800"
							>
								<button
									v-for="option in modes"
									:key="option.value"
									type="button"
									class="rounded-md px-3 py-1 text-sm transition-colors"
									:class="
										mode === option.value
											? 'bg-white text-gray-900 shadow-sm dark:bg-gray-700 dark:text-gray-100'
											: 'text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200'
									"
									@click="mode = option.value"
								>
									{{ option.label }}
								</button>
							</div>
							<Button
								v-if="snapshot.version !== page.version"
								variant="outline"
								data-testid="restore-button"
								:disabled="restoring"
								@click="confirmOpen = true"
							>
								<RotateCcw class="h-4 w-4" />
								Restore
							</Button>
						</div>

						<h2 class="mb-3 text-lg font-semibold text-ink">
							{{ snapshot.title }}
						</h2>

						<PageMarkdown
							v-if="mode === 'snapshot'"
							:markdown="snapshot.body"
							:directory="directory"
							@navigate="onNavigate"
						/>
						<template v-else>
							<p class="mb-2 text-xs text-ink-subtle" data-testid="diff-stats">
								+{{ stats.added }} / -{{ stats.removed }}
							</p>
							<PageDiff :before="diffBefore" :after="diffAfter" />
						</template>
					</template>
				</div>
			</div>

			<AlertDialog v-model:open="confirmOpen">
				<AlertDialogContent data-testid="restore-confirm">
					<AlertDialogHeader>
						<AlertDialogTitle
							>Restore version {{ selected }}?</AlertDialogTitle
						>
						<AlertDialogDescription>
							A new page version will be created with the content of version
							{{ selected }}. The current text stays in history.
						</AlertDialogDescription>
					</AlertDialogHeader>
					<AlertDialogFooter>
						<AlertDialogCancel>Cancel</AlertDialogCancel>
						<AlertDialogAction @click="restore">Restore</AlertDialogAction>
					</AlertDialogFooter>
				</AlertDialogContent>
			</AlertDialog>
		</template>
	</PageContainer>
</template>

<script lang="ts">
	import {
		getPage,
		getPageVersion,
		getPageVersions,
		type Page,
		type PageAuthor,
		type PageVersion,
		type PageVersionSnapshot,
		restorePageVersion,
	} from '@/actions/tmgr/pages';
	import AuthorBadge from '@/components/general/AuthorBadge.vue';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import PageDiff from '@/components/pages/PageDiff.vue';
	import PageMarkdown from '@/components/pages/PageMarkdown.vue';
	import { useTmgrDirectory } from '@/components/pages/useTmgrDirectory';
	import {
		AlertDialog,
		AlertDialogAction,
		AlertDialogCancel,
		AlertDialogContent,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
	} from '@/components/ui/alert-dialog';
	import { Button } from '@/components/ui/button';
	import { toAuthorRef } from '@/utils/pages/author';
	import { diffLines, diffStats } from '@/utils/pages/lineDiff';
	import type { ParsedTmgrUrl } from '@/utils/pages/tmgrLinks';
	import { RotateCcw } from 'lucide-vue-next';
	import { computed, defineComponent, onMounted, ref, watch } from 'vue';
	import { useRoute, useRouter } from 'vue-router';

	type Mode = 'snapshot' | 'current' | 'previous';

	export default defineComponent({
		name: 'PageVersions',
		components: {
			AlertDialog,
			AlertDialogAction,
			AlertDialogCancel,
			AlertDialogContent,
			AlertDialogDescription,
			AlertDialogFooter,
			AlertDialogHeader,
			AlertDialogTitle,
			AuthorBadge,
			Button,
			PageContainer,
			PageDiff,
			PageHeader,
			PageMarkdown,
			RotateCcw,
		},
		setup() {
			const route = useRoute();
			const router = useRouter();
			const workspaceCode = computed(() => String(route.params.workspace_code));
			const slug = computed(() => String(route.params.slug));
			const directory = useTmgrDirectory(() => workspaceCode.value);

			const page = ref<Page | null>(null);
			const versions = ref<PageVersion[]>([]);
			const loading = ref(false);
			const loadError = ref(false);
			const selected = ref<number | null>(null);
			const snapshot = ref<PageVersionSnapshot | null>(null);
			const previousBody = ref('');
			const snapshotLoading = ref(false);
			const mode = ref<Mode>('snapshot');
			const confirmOpen = ref(false);
			const restoring = ref(false);
			let snapshotSeq = 0;

			const modes: { value: Mode; label: string }[] = [
				{ value: 'snapshot', label: 'Snapshot' },
				{ value: 'current', label: 'Changes since this version' },
				{ value: 'previous', label: 'What this version changed' },
			];

			const diffBefore = computed(() =>
				mode.value === 'current'
					? snapshot.value?.body ?? ''
					: previousBody.value,
			);
			const diffAfter = computed(() =>
				mode.value === 'current'
					? page.value?.body ?? ''
					: snapshot.value?.body ?? '',
			);
			const stats = computed(() =>
				diffStats(diffLines(diffBefore.value, diffAfter.value)),
			);

			const authorOf = (author: PageAuthor) => toAuthorRef(author);
			const formatDate = (value: string) => new Date(value).toLocaleString();

			const select = async (version: number) => {
				selected.value = version;
				const seq = ++snapshotSeq;
				snapshotLoading.value = true;
				try {
					const loaded = await getPageVersion(page.value!.id, version);
					const older = versions.value.find((v) => v.version < version);
					const before = older
						? await getPageVersion(page.value!.id, older.version)
						: null;
					if (seq !== snapshotSeq) return;
					snapshot.value = loaded;
					previousBody.value = before?.body ?? '';
				} catch {
					if (seq === snapshotSeq) snapshot.value = null;
				} finally {
					if (seq === snapshotSeq) snapshotLoading.value = false;
				}
			};

			const load = async () => {
				loading.value = true;
				loadError.value = false;
				try {
					const [loaded, list] = await Promise.all([
						getPage(slug.value),
						getPageVersions(slug.value),
					]);
					page.value = loaded;
					versions.value = list;
					const wanted = Number(route.query.v);
					const initial = list.find((v) => v.version === wanted) ?? list[0];
					if (initial) await select(initial.version);
				} catch {
					loadError.value = true;
				} finally {
					loading.value = false;
				}
			};

			const restore = async () => {
				if (!page.value || !selected.value) return;
				restoring.value = true;
				try {
					const restored = await restorePageVersion(
						page.value.id,
						selected.value,
					);
					await router.push(`/${workspaceCode.value}/pages/${restored.slug}`);
				} catch {
					loadError.value = true;
				} finally {
					restoring.value = false;
					confirmOpen.value = false;
				}
			};

			const onNavigate = async (parsed: ParsedTmgrUrl) => {
				if (parsed.form === 'storage' && parsed.kind === 'file') return;
				if (parsed.form === 'storage') {
					await directory.ensure([{ kind: parsed.kind, id: parsed.id }]);
				}
				const target = directory.pathFor(parsed);
				if (target) void router.push(target);
			};

			watch(slug, () => void load());
			onMounted(load);

			return {
				workspaceCode,
				directory,
				page,
				versions,
				loading,
				loadError,
				load,
				selected,
				snapshot,
				snapshotLoading,
				mode,
				modes,
				diffBefore,
				diffAfter,
				stats,
				confirmOpen,
				restoring,
				authorOf,
				formatDate,
				select,
				restore,
				onNavigate,
			};
		},
	});
</script>
