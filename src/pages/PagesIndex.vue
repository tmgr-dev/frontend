<template>
	<PageContainer
		:class="zone.dragging.value ? 'rounded-card ring-2 ring-blue-500' : ''"
		data-testid="pages-index"
		v-on="zone.handlers"
	>
		<PageHeader title="Pages">
			<template #actions>
				<router-link
					:to="`/${workspaceCode}/pages/_/trash`"
					class="inline-flex h-9 items-center gap-2 rounded-md px-3 text-sm text-ink-subtle hover:bg-surface-hover hover:text-ink"
				>
					<Trash2 class="h-4 w-4" />
					Trash
				</router-link>
				<button
					type="button"
					class="inline-flex h-9 items-center gap-2 rounded-md px-3 text-sm text-ink-subtle hover:bg-surface-hover hover:text-ink"
					data-testid="pages-import"
					@click="io.openImport(null)"
				>
					<Upload class="h-4 w-4" />
					Import
				</button>
				<button
					type="button"
					class="inline-flex h-9 items-center gap-2 rounded-md px-3 text-sm text-ink-subtle hover:bg-surface-hover hover:text-ink disabled:opacity-50"
					:disabled="io.busy.value"
					data-testid="pages-export-all"
					@click="io.exportWorkspace()"
				>
					<Download class="h-4 w-4" />
					Export all
				</button>
				<button
					type="button"
					class="inline-flex h-9 items-center gap-2 rounded-md bg-tmgr-blue px-4 text-sm font-medium text-white hover:bg-tmgr-blue/90 dark:bg-blue-600 dark:hover:bg-blue-500"
					@click="create()"
				>
					<Plus class="h-4 w-4" />
					Create page
				</button>
			</template>
		</PageHeader>

		<p v-if="!loaded" class="text-sm text-ink-subtle">Loading…</p>
		<p v-else-if="failed" class="text-sm text-destructive">
			Failed to load pages
		</p>
		<div
			v-else-if="!pages.length"
			class="flex flex-col items-center justify-center gap-3 py-16 text-center"
		>
			<FileText class="h-12 w-12 text-gray-400 dark:text-gray-500" />
			<p class="text-xl font-bold text-gray-700 dark:text-gray-300">
				No pages yet
			</p>
			<p class="text-sm text-gray-500 dark:text-gray-400">
				Create your first page for notes and documentation.
			</p>
			<button
				type="button"
				class="mt-2 inline-flex h-10 items-center rounded-md bg-tmgr-blue px-6 text-sm font-semibold text-white hover:bg-tmgr-blue/90 dark:bg-blue-600 dark:hover:bg-blue-500"
				@click="create()"
			>
				Create page
			</button>
		</div>
		<div v-else class="space-y-8">
			<section v-if="pinned.length">
				<h2 class="mb-2 text-sm font-semibold text-ink-subtle">Pinned</h2>
				<ul class="divide-y divide-line overflow-hidden rounded-card border border-line bg-surface">
					<li v-for="page in pinned" :key="page.id">
						<PageRow :page="page" :workspace-code="workspaceCode" />
					</li>
				</ul>
			</section>
			<section v-if="roots.length">
				<h2 class="mb-2 text-sm font-semibold text-ink-subtle">Root pages</h2>
				<ul class="divide-y divide-line overflow-hidden rounded-card border border-line bg-surface">
					<li v-for="page in roots" :key="page.id">
						<PageRow :page="page" :workspace-code="workspaceCode" />
					</li>
				</ul>
			</section>
		</div>
	</PageContainer>
</template>

<script lang="ts">
	import { getPagesTree, type PageSummary } from '@/actions/tmgr/pages';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import PageRow from '@/components/pagesNav/PageRow.vue';
	import { usePagesActions } from '@/composable/usePagesActions';
	import {
		useFileDrop,
		usePagesMarkdownIo,
	} from '@/composable/usePagesMarkdownIo';
	import { childrenOf } from '@/utils/pagesTree';
	import { Download, FileText, Plus, Trash2, Upload } from 'lucide-vue-next';
	import { computed, defineComponent, onMounted, ref, watch } from 'vue';
	import { useRoute } from 'vue-router';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'PagesIndex',
		components: {
			Download,
			FileText,
			PageContainer,
			PageHeader,
			PageRow,
			Plus,
			Trash2,
			Upload,
		},
		setup() {
			const route = useRoute();
			const store = useStore();
			const workspaceCode = computed(() => String(route.params.workspace_code));
			const actions = usePagesActions(() => workspaceCode.value);
			const io = usePagesMarkdownIo(() => workspaceCode.value);
			const zone = useFileDrop((files) => io.openImport(null, files));
			const pages = ref<PageSummary[]>([]);
			const loaded = ref(false);
			const failed = ref(false);

			const load = async (useCache = true) => {
				try {
					pages.value = await getPagesTree(useCache);
					failed.value = false;
				} catch {
					failed.value = true;
				} finally {
					loaded.value = true;
				}
			};

			onMounted(() => load());
			watch(
				() => store.state.pagesEvent?.seq,
				() => load(false),
			);

			const pinned = computed(() =>
				pages.value.filter((page) => page.pinned),
			);
			const roots = computed(() =>
				childrenOf(pages.value, null).filter((page) => !page.pinned),
			);

			return {
				workspaceCode,
				io,
				zone,
				pages,
				loaded,
				failed,
				pinned,
				roots,
				create: () => actions.create('plain', null),
			};
		},
	});
</script>
