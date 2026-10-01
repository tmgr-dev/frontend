<template>
	<FeatureGate
		feature-key="pages"
		title="Pages"
		description="Notes, workspace context and documentation in a page tree that people and agents both edit."
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
