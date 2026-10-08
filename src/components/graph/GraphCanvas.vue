<template>
	<canvas
		ref="canvasEl"
		class="block h-full w-full touch-none select-none"
		role="img"
		:aria-label="label"
	/>
</template>

<script lang="ts">
	import type { GraphResult } from '@/types/graph';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
		watch,
		type PropType,
	} from 'vue';
	import { GraphEngine } from './graphEngine';
	import { useDocumentTheme } from './useDocumentTheme';

	export default defineComponent({
		name: 'GraphCanvas',
		props: {
			result: {
				type: Object as PropType<GraphResult | null>,
				default: null,
			},
			selectedId: { type: String as PropType<string | null>, default: null },
			compact: { type: Boolean, default: false },
			topInset: { type: Number, default: 0 },
			wheelZoom: {
				type: String as PropType<'always' | 'modifier'>,
				default: 'always',
			},
		},
		emits: ['select', 'center', 'hover'],
		setup(props, { emit, expose }) {
			const canvasEl = ref<HTMLCanvasElement | null>(null);
			const { version } = useDocumentTheme();
			let engine: GraphEngine | null = null;

			const label = computed(() => {
				const count = props.result?.nodes.length ?? 0;
				return `Graph of ${count} connected items`;
			});

			onMounted(() => {
				if (!canvasEl.value) return;
				const reducedMotion =
					typeof window.matchMedia === 'function' &&
					window.matchMedia('(prefers-reduced-motion: reduce)').matches;
				engine = new GraphEngine(
					canvasEl.value,
					{
						compact: props.compact,
						wheelZoom: props.wheelZoom,
						reducedMotion,
					},
					{
						onSelect: (id) => emit('select', id),
						onCenter: (id) => emit('center', id),
						onHover: (id) => emit('hover', id),
					},
				);
				engine.setTopInset(props.topInset);
				if (props.result) engine.setData(props.result);
				engine.setSelected(props.selectedId);
			});

			onBeforeUnmount(() => {
				engine?.destroy();
				engine = null;
			});

			watch(
				() => props.result,
				(next) => {
					if (next) engine?.setData(next);
				},
			);
			watch(
				() => props.selectedId,
				(id) => engine?.setSelected(id),
			);
			watch(
				() => props.topInset,
				(px) => engine?.setTopInset(px),
			);
			watch(version, () => engine?.refreshTheme());

			expose({
				fit: () => engine?.fit(),
				zoomBy: (factor: number) => engine?.zoomBy(factor),
			});

			return { canvasEl, label };
		},
	});
</script>
