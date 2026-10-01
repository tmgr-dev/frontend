<template>
	<FeatureGate
		feature-key="pages"
		title="Страницы"
		description="Заметки, контекст воркспейса и документация в дереве страниц, которые редактируют и люди, и агенты."
		:icon="FileText"
	>
		<template #preview>
			<PagesPreview />
		</template>
		<router-view />
	</FeatureGate>
</template>

<script lang="ts">
	import FeatureGate from '@/components/general/FeatureGate.vue';
	import PagesPreview from '@/components/previews/PagesPreview.vue';
	import { FileText } from 'lucide-vue-next';
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'PagesLayout',
		components: { FeatureGate, PagesPreview },
		setup() {
			return { FileText };
		},
	});
</script>
