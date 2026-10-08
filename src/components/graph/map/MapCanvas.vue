<template>
	<canvas
		ref="canvasEl"
		class="block h-full w-full touch-none select-none"
		style="cursor: grab"
		role="img"
		:aria-label="label"
		data-testid="map-canvas"
		:data-settled="settled ? '1' : '0'"
		:data-layout-ms="layoutMs"
	/>
</template>

<script lang="ts">
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
		watch,
		type PropType,
	} from 'vue';
	import { useDocumentTheme } from '../useDocumentTheme';
	import { MapEngine } from './MapEngine';
	import type { PreparedMap } from './mapLogic';

	export default defineComponent({
		name: 'MapCanvas',
		props: {
			prepared: {
				type: Object as PropType<PreparedMap | null>,
				default: null,
			},
			group: { type: Boolean, default: true },
			showOrphans: { type: Boolean, default: true },
			cursor: { type: Number, default: Infinity },
			matches: {
				type: Array as PropType<number[] | null>,
				default: null,
			},
			pinned: {
				type: Array as PropType<number[] | null>,
				default: null,
			},
		},
		emits: ['select', 'hover', 'layout'],
		setup(props, { emit, expose }) {
			const canvasEl = ref<HTMLCanvasElement | null>(null);
			const settled = ref(false);
			const layoutMs = ref(0);
			const { version } = useDocumentTheme();
			let engine: MapEngine | null = null;

			const label = computed(() => {
				const p = props.prepared;
				if (!p) return 'Map of tasks and pages grouped by category';
				const names = p.clusters.map((c) => c.title).join(', ');
				const bottleneck =
					p.bottleneck >= 0
						? ` Bottleneck: ${
								p.nodes[p.bottleneck].key ?? p.labels[p.bottleneck]
						  } blocks ${p.bottleneckBlocks}.`
						: '';
				return `Map of ${p.nodes.length} tasks and pages in ${p.clusters.length} clusters: ${names}.${bottleneck} Use the search box to move around the map by keyboard.`;
			});

			const load = () => {
				if (!engine || !props.prepared) return;
				settled.value = false;
				engine.setData(props.prepared, props.group, props.showOrphans);
				engine.setCursor(props.cursor);
				engine.setSearch(props.matches);
				engine.pin(props.pinned);
			};

			onMounted(() => {
				if (!canvasEl.value) return;
				engine = new MapEngine(canvasEl.value, {
					onSelect: (id) => emit('select', id),
					onHover: (i) => emit('hover', i),
					onLayout: (info) => {
						settled.value = true;
						layoutMs.value = Math.round(info.ms);
						emit('layout', info);
					},
				});
				load();
			});
			onBeforeUnmount(() => {
				engine?.destroy();
				engine = null;
			});

			watch(() => props.prepared, load);
			watch(
				() => props.group,
				(v) => {
					settled.value = false;
					engine?.setGroup(v);
				},
			);
			watch(
				() => props.showOrphans,
				(v) => engine?.setShowOrphans(v),
			);
			watch(
				() => props.cursor,
				(v) => engine?.setCursor(v),
			);
			watch(
				() => props.matches,
				(v) => engine?.setSearch(v),
			);
			watch(
				() => props.pinned,
				(v) => engine?.pin(v),
			);
			watch(version, () => engine?.refreshTheme());

			expose({
				fit: () => engine?.fit(),
				zoomBy: (factor: number) => engine?.zoomBy(factor),
				focusIndices: (indices: number[]) => engine?.focusIndices(indices),
			});

			return { canvasEl, label, settled, layoutMs };
		},
	});
</script>
