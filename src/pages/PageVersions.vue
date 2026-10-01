<template>
	<PageContainer width="narrow">
		<PageHeader title="Версии" />
		<ul class="space-y-1 text-sm text-ink">
			<li v-for="version in versions" :key="version.version">
				v{{ version.version }} — {{ version.title }}
			</li>
		</ul>
	</PageContainer>
</template>

<script lang="ts">
	import { getPageVersions, type PageVersion } from '@/actions/tmgr/pages';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import { defineComponent, ref, watch } from 'vue';
	import { useRoute } from 'vue-router';

	export default defineComponent({
		name: 'PageVersions',
		components: { PageContainer, PageHeader },
		setup() {
			const route = useRoute();
			const versions = ref<PageVersion[]>([]);

			watch(
				() => route.params.slug as string,
				async (slug) => {
					try {
						versions.value = await getPageVersions(slug);
					} catch {
						versions.value = [];
					}
				},
				{ immediate: true },
			);

			return { versions };
		},
	});
</script>
