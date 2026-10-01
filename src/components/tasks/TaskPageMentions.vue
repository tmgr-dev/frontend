<template>
	<section v-if="pages.length" data-testid="task-page-mentions">
		<h3
			class="mb-1.5 text-2xs font-bold uppercase tracking-wide text-ink-subtle"
		>
			Упоминается на страницах
		</h3>
		<div class="flex flex-wrap gap-1.5">
			<router-link
				v-for="page in pages"
				:key="page.id"
				:to="pageUrl(workspaceCode, page.slug)"
				class="inline-flex items-center gap-1 rounded-full bg-blue-100 px-2.5 py-0.5 text-xs text-blue-800 hover:underline dark:bg-blue-900/40 dark:text-blue-200"
			>
				<FileText class="h-3 w-3" />
				{{ page.title }}
			</router-link>
		</div>
	</section>
</template>

<script lang="ts">
	import { getTaskPages, type PageSummary } from '@/actions/tmgr/pages';
	import store from '@/store';
	import type { RootState } from '@/types/store';
	import { shouldReloadTaskPages } from '@/utils/pages/taskMentions';
	import { pageUrl } from '@/utils/pagesTree';
	import { FileText } from 'lucide-vue-next';
	import { computed, defineComponent, onMounted, ref, watch } from 'vue';

	export default defineComponent({
		name: 'TaskPageMentions',
		components: { FileText },
		props: {
			taskId: { type: Number, required: true },
		},
		setup(props) {
			const pages = ref<PageSummary[]>([]);
			const workspaceCode = computed(
				() => store.getters.currentWorkspace?.code ?? '',
			);
			let seq = 0;

			const load = async () => {
				const current = ++seq;
				try {
					const list = await getTaskPages(props.taskId);
					if (current === seq) pages.value = list;
				} catch {
					if (current === seq) pages.value = [];
				}
			};

			onMounted(load);
			watch(() => props.taskId, load);
			watch(
				() => (store.state as RootState).pagesEvent?.seq,
				() => {
					const event = (store.state as RootState).pagesEvent;
					if (shouldReloadTaskPages(event, props.taskId)) void load();
				},
			);

			return { pages, workspaceCode, pageUrl };
		},
	});
</script>
