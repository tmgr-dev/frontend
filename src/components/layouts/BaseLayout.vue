<template>
	<PageContainer :width="width">
		<PageHeader
			v-if="$slots.header || title"
			:title="title"
			:subtitle="subtitle"
			:back="back"
		>
			<template v-if="$slots.header" #title>
				<slot name="header" />
			</template>
			<template v-if="$slots['header-actions']" #actions>
				<slot name="header-actions" />
			</template>
		</PageHeader>

		<slot name="action" />

		<div>
			<slot name="body" />
		</div>

		<slot name="footer" />
	</PageContainer>
</template>

<script>
	import { defineComponent } from 'vue';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';

	export default defineComponent({
		name: 'BaseLayout',
		components: { PageContainer, PageHeader },
		props: {
			noCopyright: {
				type: Boolean,
				required: false,
				default: false,
			},
			width: {
				type: String,
				default: 'wide',
			},
			title: {
				type: String,
				default: '',
			},
			subtitle: {
				type: String,
				default: '',
			},
			back: {
				type: [String, Object],
				default: null,
			},
		},
	});
</script>
