<template>
	<div :class="classes" :data-page-width="width">
		<div
			v-if="width === 'narrow'"
			class="flex w-full min-w-0 max-w-[46rem] flex-1 flex-col md:max-w-[45rem]"
		>
			<slot />
		</div>
		<slot v-else />
	</div>
</template>

<script>
	import { computed, defineComponent } from 'vue';

	const FRAME =
		'mx-auto flex w-full min-w-0 max-w-7xl flex-1 flex-col px-4 pb-10 pt-4 md:px-6 md:pt-6';

	const WIDTHS = {
		full: 'flex min-w-0 flex-1 flex-col',
		wide: FRAME,
		narrow: FRAME,
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
