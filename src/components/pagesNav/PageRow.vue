<template>
	<router-link
		:to="url"
		class="flex items-center gap-3 px-4 py-3 hover:bg-surface-hover"
	>
		<FileText class="h-4 w-4 shrink-0 text-ink-subtle" />
		<span class="min-w-0 flex-1 truncate text-sm font-medium text-ink">{{
			page.title
		}}</span>
		<span class="shrink-0 text-xs text-ink-subtle">{{ typeLabel }}</span>
		<slot />
	</router-link>
</template>

<script lang="ts">
	import type { PageSummary } from '@/actions/tmgr/pages';
	import { pageTypeLabel } from '@/utils/pagesSearch';
	import { pageUrl } from '@/utils/pagesTree';
	import { FileText } from 'lucide-vue-next';
	import { computed, defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'PageRow',
		components: { FileText },
		props: {
			page: { type: Object as PropType<PageSummary>, required: true },
			workspaceCode: { type: String, required: true },
		},
		setup(props) {
			return {
				url: computed(() => pageUrl(props.workspaceCode, props.page.slug)),
				typeLabel: computed(() => pageTypeLabel(props.page.type)),
			};
		},
	});
</script>
