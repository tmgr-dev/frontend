<template>
	<div :class="classes" :data-page-width="width">
		<slot />
	</div>
</template>

<script>
	import { computed, defineComponent } from 'vue';

	const WIDTHS = {
		full: 'flex min-w-0 flex-1 flex-col',
		wide: 'flex w-full min-w-0 max-w-7xl flex-1 flex-col px-4 pb-10 pt-4 md:px-6 md:pt-6',
		narrow:
			'flex w-full min-w-0 max-w-3xl flex-1 flex-col px-4 pb-10 pt-4 md:px-6 md:pt-6',
	};

	export default defineComponent({
		name: 'PageContainer',
		props: {
			width: {
				type: String,
				default: 'wide',
				validator: (value) => Object.keys(WIDTHS).includes(value),
			},
		},
		setup(props) {
			const classes = computed(() => WIDTHS[props.width] || WIDTHS.wide);

			return { classes };
		},
	});
</script>
