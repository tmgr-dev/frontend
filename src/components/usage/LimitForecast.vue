<template>
	<p class="mt-1 text-xs" :class="tone" data-testid="limit-forecast">
		{{ text }}
	</p>
</template>

<script lang="ts">
	import type { UsageLimit } from '@/usage/api';
	import { forecastLimit, forecastText } from '@/usage/forecast';
	import { computed, defineComponent, type PropType } from 'vue';

	const TONES = {
		runs_out: 'text-amber-700 dark:text-amber-400',
		reached: 'text-red-600 dark:text-red-400',
		maybe_reached: 'text-amber-700 dark:text-amber-400',
		lasts: 'text-ink-subtle',
		none: 'text-ink-subtle',
	};

	export default defineComponent({
		name: 'LimitForecast',
		props: {
			limit: { type: Object as PropType<UsageLimit>, required: true },
		},
		setup(props) {
			const forecast = computed(() => forecastLimit(props.limit));
			return {
				text: computed(() => forecastText(forecast.value)),
				tone: computed(() => TONES[forecast.value.kind]),
			};
		},
	});
</script>
