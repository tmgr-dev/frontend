<template>
	<PageContainer
		:class="zone.dragging.value ? 'rounded-card ring-2 ring-blue-500' : ''"
		data-testid="pages-index"
		v-on="zone.handlers"
	>
		<PageHeader title="Pages">
			<template #actions>
				<router-link
					:to="`/${workspaceCode}/pages/_/archive`"
					class="inline-flex h-9 items-center gap-2 rounded-md px-3 text-sm text-ink-subtle hover:bg-surface-hover hover:text-ink"
				>
					<Archive class="h-4 w-4" />
					Archive
				</router-link>
				<button
					type="button"
					class="inline-flex h-9 items-center gap-2 rounded-md px-3 text-sm text-ink-subtle hover:bg-surface-hover hover:text-ink"
					data-testid="pages-import"
					data-tour="pages.import"
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
					data-tour="pages.export"
					@click="io.exportWorkspace()"
				>
					<Download class="h-4 w-4" />
					Export all
				</button>
				<button
					type="button"
					class="inline-flex h-9 items-center gap-2 rounded-md bg-tmgr-blue px-4 text-sm font-medium text-white hover:bg-tmgr-blue/90 dark:bg-blue-600 dark:hover:bg-blue-500"
					data-tour="pages.new"
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
				<ul
					class="divide-y divide-line overflow-hidden rounded-card border border-line bg-surface"
				>
					<li v-for="page in pinned" :key="page.id">
						<PageRow :page="page" :workspace-code="workspaceCode" />
					</li>
				</ul>
			</section>
			<section data-testid="pages-index-tree" data-pages-tree="page">
				<h2 class="mb-2 text-sm font-semibold text-ink-subtle">All pages</h2>
				<div class="rounded-card border border-line bg-surface p-2">
					<Draggable
						:model-value="tree"
						group="pages"
						item-key="id"
						class="min-h-[8px]"
						:force-fallback="true"
						:fallback-on-body="true"
						:fallback-tolerance="3"
						:delay="150"
						:delay-on-touch-only="true"
						@start="onDragStart"
						@end="dragging = null"
						@change="(event) => onChange(null, event)"
					>
						<template #item="{ element }">
							<PagesTreeNode :node="element" />
						</template>
					</Draggable>
				</div>
			</section>
		</div>
		<PagesTreeDialogs />
	</PageContainer>
</template>

<script lang="ts">
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import PageRow from '@/components/pagesNav/PageRow.vue';
	import PagesTreeDialogs from '@/components/pagesNav/PagesTreeDialogs.vue';
	import PagesTreeNode from '@/components/pagesNav/PagesTreeNode.vue';
	import { useFileDrop } from '@/composable/usePagesMarkdownIo';
	import { usePagesTreeHost } from '@/composable/usePagesTreeHost';
	import { Archive, Download, FileText, Plus, Upload } from 'lucide-vue-next';
	import { computed, defineComponent } from 'vue';
	import { useRoute } from 'vue-router';
	import Draggable from 'vuedraggable';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'PagesIndex',
		components: {
			Download,
			Draggable,
			FileText,
			PageContainer,
			PageHeader,
			PageRow,
			PagesTreeDialogs,
			PagesTreeNode,
			Plus,
			Archive,
			Upload,
		},
		setup() {
			const route = useRoute();
			const store = useStore();
			const workspaceCode = computed(() => String(route.params.workspace_code));
			const workspaceId = computed(() => {
				const id = Number(store.getters.currentWorkspaceId);
				return Number.isFinite(id) && id > 0 ? id : null;
			});
			const { tree, actions, io, dragging, onDragStart, onChange } =
				usePagesTreeHost(workspaceId, workspaceCode, 'page');
			const zone = useFileDrop((files) => io.openImport(null, files));

			const pinned = computed(() =>
				tree.pages.value.filter((page) => page.pinned),
			);

			return {
				workspaceCode,
				io,
				zone,
				pages: tree.pages,
				tree: tree.tree,
				loaded: tree.loaded,
				failed: tree.failed,
				pinned,
				dragging,
				onDragStart,
				onChange,
				create: () => actions.create('plain', null),
			};
		},
	});
</script>
