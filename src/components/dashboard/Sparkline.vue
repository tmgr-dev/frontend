<template>
	<svg
		v-if="values.length >= 2"
		data-sparkline
		aria-hidden="true"
		focusable="false"
		:class="['block w-full', toneClass]"
		:style="{ height: `${height}px` }"
		:viewBox="`0 0 100 ${height}`"
		preserveAspectRatio="none"
	>
		<defs>
			<linearGradient :id="gradientId" x1="0" y1="0" x2="0" y2="1">
				<stop offset="0%" stop-color="currentColor" stop-opacity="0.35" />
				<stop offset="100%" stop-color="currentColor" stop-opacity="0" />
			</linearGradient>
		</defs>
		<path :d="areaPath" :fill="`url(#${gradientId})`" stroke="none" />
		<path
			:d="linePath"
			fill="none"
			stroke="currentColor"
			stroke-width="2"
			stroke-linecap="round"
			stroke-linejoin="round"
			vector-effect="non-scaling-stroke"
		/>
	</svg>
</template>

<script lang="ts">
	import type { DashboardTone } from '@/types/dashboard';
	import { computed, defineComponent, type PropType } from 'vue';

	const TONE_LINE: Record<DashboardTone, string> = {
		brand: 'text-brand',
		done: 'text-status-done',
		testing: 'text-status-testing',
		progress: 'text-status-progress',
	};

	// Gradient ids are document-global: a shared id breaks every sparkline once the first one unmounts.
	let instanceCounter = 0;

	export default defineComponent({
		name: 'Sparkline',
		props: {
			values: { type: Array as PropType<number[]>, required: true },
			tone: { type: String as PropType<DashboardTone>, required: true },
			height: { type: Number, default: 40 },
		},
		setup(props) {
			instanceCounter += 1;
			const gradientId = `sparkline-gradient-${instanceCounter}`;
			const toneClass = computed(() => TONE_LINE[props.tone]);

			const points = computed(() => {
				const values = props.values;
				const pad = 2;
				const bottom = props.height - pad;
				const usable = props.height - pad * 2;
				const max = Math.max(0, ...values);
				const last = values.length - 1;
				return values.map((v, i) => ({
					x: last > 0 ? (i / last) * 100 : 0,
					y: max > 0 ? bottom - (Math.max(0, v) / max) * usable : bottom,
				}));
			});

			const linePath = computed(() =>
				points.value.map((p, i) => `${i ? 'L' : 'M'}${p.x},${p.y}`).join(' '),
			);
			const areaPath = computed(
				() => `${linePath.value} L100,${props.height} L0,${props.height} Z`,
			);

			return { gradientId, toneClass, linePath, areaPath };
		},
	});
</script>
