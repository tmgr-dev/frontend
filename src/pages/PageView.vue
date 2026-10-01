<template>
	<PageContainer width="narrow">
		<p v-if="error" class="text-sm text-destructive">{{ error }}</p>
		<template v-else-if="page">
			<PageHeader :title="page.title" />
			<pre class="whitespace-pre-wrap break-words text-sm text-ink">{{
				page.body
			}}</pre>
		</template>
	</PageContainer>
</template>

<script lang="ts">
	import { getPage, type Page } from '@/actions/tmgr/pages';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import { defineComponent, ref, watch } from 'vue';
	import { useRoute } from 'vue-router';

	export default defineComponent({
		name: 'PageView',
		components: { PageContainer, PageHeader },
		setup() {
			const route = useRoute();
			const page = ref<Page | null>(null);
			const error = ref('');

			watch(
				() => route.params.slug as string,
				async (slug) => {
					error.value = '';
					try {
						page.value = await getPage(slug);
					} catch {
						page.value = null;
						error.value = 'Страница не найдена';
					}
				},
				{ immediate: true },
			);

			return { page, error };
		},
	});
</script>
