<template>
	<PageContainer>
		<PageHeader
			title="Pages trash"
			:back="`/${workspaceCode}/pages`"
			back-label="Pages"
		/>
		<p v-if="!loaded" class="text-sm text-ink-subtle">Loading…</p>
		<p v-else-if="failed" class="text-sm text-destructive">
			Failed to load trash
		</p>
		<p v-else-if="!pages.length" class="text-sm text-ink-subtle">
			Trash is empty.
		</p>
		<ul
			v-else
			class="divide-y divide-line overflow-hidden rounded-card border border-line bg-surface"
		>
			<li
				v-for="page in pages"
				:key="page.id"
				class="flex items-center gap-3 px-4 py-3"
			>
				<Trash2 class="h-4 w-4 shrink-0 text-ink-subtle" />
				<div class="min-w-0 flex-1">
					<p class="truncate text-sm font-medium text-ink">{{ page.title }}</p>
					<p class="text-xs text-ink-subtle">
						Deleted {{ formatDeleted(page.deleted_at) }}
					</p>
				</div>
				<button
					type="button"
					class="inline-flex h-8 items-center gap-1.5 rounded-md border border-gray-300 px-3 text-sm text-ink hover:bg-surface-hover disabled:opacity-50 dark:border-gray-600"
					:disabled="restoring === page.id"
					@click="restore(page)"
				>
					<Undo2 class="h-4 w-4" />
					Restore
				</button>
			</li>
		</ul>
	</PageContainer>
</template>

<script lang="ts">
	import {
		getPagesTrash,
		restorePage,
		type PageSummary,
	} from '@/actions/tmgr/pages';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import { useToast } from '@/components/ui/toast';
	import { pageUrl } from '@/utils/pagesTree';
	import { Trash2, Undo2 } from 'lucide-vue-next';
	import { computed, defineComponent, onMounted, ref, watch } from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'PagesTrash',
		components: { PageContainer, PageHeader, Trash2, Undo2 },
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

			const formatDeleted = (value?: string | null) =>
				value ? new Date(value).toLocaleString() : '';

			return { workspaceCode, pages, loaded, failed, restoring, restore, formatDeleted };
		},
	});
</script>
