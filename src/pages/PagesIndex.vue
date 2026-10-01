<template>
	<PageContainer>
		<PageHeader title="Страницы">
			<template #actions>
				<router-link
					:to="`/${workspaceCode}/pages/_/trash`"
					class="inline-flex h-9 items-center gap-2 rounded-md px-3 text-sm text-ink-subtle hover:bg-surface-hover hover:text-ink"
				>
					<Trash2 class="h-4 w-4" />
					Корзина
				</router-link>
				<button
					type="button"
					class="inline-flex h-9 items-center gap-2 rounded-md bg-tmgr-blue px-4 text-sm font-medium text-white hover:bg-tmgr-blue/90 dark:bg-blue-600 dark:hover:bg-blue-500"
					@click="create()"
				>
					<Plus class="h-4 w-4" />
					Создать страницу
				</button>
			</template>
		</PageHeader>

		<p v-if="!loaded" class="text-sm text-ink-subtle">Загрузка…</p>
		<p v-else-if="failed" class="text-sm text-destructive">
			Не удалось загрузить страницы
		</p>
		<div
			v-else-if="!pages.length"
			class="flex flex-col items-center justify-center gap-3 py-16 text-center"
		>
			<FileText class="h-12 w-12 text-gray-400 dark:text-gray-500" />
			<p class="text-xl font-bold text-gray-700 dark:text-gray-300">
				Страниц пока нет
			</p>
			<p class="text-sm text-gray-500 dark:text-gray-400">
				Создайте первую страницу для заметок и документации.
			</p>
			<button
				type="button"
				class="mt-2 inline-flex h-10 items-center rounded-md bg-tmgr-blue px-6 text-sm font-semibold text-white hover:bg-tmgr-blue/90 dark:bg-blue-600 dark:hover:bg-blue-500"
				@click="create()"
			>
				Создать страницу
			</button>
		</div>
		<div v-else class="space-y-8">
			<section v-if="pinned.length">
				<h2 class="mb-2 text-sm font-semibold text-ink-subtle">Закреплённые</h2>
				<ul class="divide-y divide-gray-200 rounded-lg border border-gray-200 dark:divide-gray-700 dark:border-gray-700">
					<li v-for="page in pinned" :key="page.id">
						<PageRow :page="page" :workspace-code="workspaceCode" />
					</li>
				</ul>
			</section>
			<section v-if="roots.length">
				<h2 class="mb-2 text-sm font-semibold text-ink-subtle">Корневые страницы</h2>
				<ul class="divide-y divide-gray-200 rounded-lg border border-gray-200 dark:divide-gray-700 dark:border-gray-700">
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
	import { childrenOf } from '@/utils/pagesTree';
	import { FileText, Plus, Trash2 } from 'lucide-vue-next';
	import { computed, defineComponent, onMounted, ref, watch } from 'vue';
	import { useRoute } from 'vue-router';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'PagesIndex',
		components: { FileText, PageContainer, PageHeader, PageRow, Plus, Trash2 },
		setup() {
			const route = useRoute();
			const store = useStore();
			const workspaceCode = computed(() => String(route.params.workspace_code));
			const actions = usePagesActions(() => workspaceCode.value);
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
