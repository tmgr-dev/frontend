<template>
	<div
		ref="root"
		class="relative overflow-hidden border border-line bg-surface-sunken"
		:class="
			mode === 'mini' ? 'h-[280px] rounded-xl' : 'h-full w-full rounded-xl'
		"
		data-testid="graph-neighborhood"
	>
		<GraphCanvas
			v-if="shown"
			ref="canvas"
			:result="shown"
			:selected-id="selectedId"
			:compact="mode === 'mini'"
			:top-inset="inset"
			:wheel-zoom="mode === 'mini' ? 'modifier' : 'always'"
			@select="$emit('select', $event)"
			@center="$emit('center', $event)"
		/>

		<div
			v-if="!narrow"
			ref="legendEl"
			class="pointer-events-none absolute left-3 top-3 flex max-w-[78%] flex-wrap gap-1.5 text-ink-muted"
			:class="mode === 'mini' ? 'text-2xs' : 'text-xs'"
			data-testid="graph-legend"
		>
			<span
				v-for="type in legend"
				:key="type"
				class="bg-surface/90 flex items-center gap-1.5 rounded-full border border-line px-2.5 py-1"
			>
				<span class="legend-glyph" :style="glyphStyle(type)" />
				{{ styles[type].label }}
			</span>
		</div>

		<button
			v-if="mode === 'mini'"
			type="button"
			class="bg-surface/90 absolute right-3 top-3 inline-flex items-center gap-1.5 rounded-lg border border-line px-2.5 py-1 text-xs font-medium text-ink hover:bg-surface-hover"
			data-testid="graph-expand"
			@click="$emit('expand')"
		>
			<Maximize2 class="h-3.5 w-3.5" />
			Expand
		</button>

		<div
			v-else-if="shown"
			class="bg-surface/90 absolute bottom-3 right-3 flex flex-col overflow-hidden rounded-lg border border-line"
		>
			<button
				type="button"
				class="p-2 text-ink-muted hover:bg-surface-hover"
				aria-label="Zoom in"
				@click="zoom(1.3)"
			>
				<Plus class="h-4 w-4" />
			</button>
			<button
				type="button"
				class="border-t border-line p-2 text-ink-muted hover:bg-surface-hover"
				aria-label="Zoom out"
				@click="zoom(1 / 1.3)"
			>
				<Minus class="h-4 w-4" />
			</button>
			<button
				type="button"
				class="border-t border-line p-2 text-ink-muted hover:bg-surface-hover"
				aria-label="Fit to view"
				@click="fit"
			>
				<Scan class="h-4 w-4" />
			</button>
		</div>

		<p
			v-if="mode === 'full'"
			class="pointer-events-none absolute bottom-3 left-3 max-w-[70%] text-xs text-ink-muted"
		>
			Inner ring: direct links · Outer ring: 2 steps away, dimmed · Dashed: weak
			link
		</p>

		<p
			v-if="truncatedNotice"
			class="bg-surface/90 absolute bottom-3 rounded-md border border-line px-2 py-1 text-xs text-ink-muted"
			:class="mode === 'mini' ? 'left-3' : 'left-1/2 -translate-x-1/2'"
			data-testid="graph-truncated"
		>
			{{ truncatedNotice }}
		</p>

		<div
			v-if="empty"
			class="absolute inset-0 flex items-center justify-center px-6 text-center text-sm text-ink-muted"
			data-testid="graph-empty"
		>
			No links yet — relate tasks, link pages or assign people to see the graph
		</div>
		<div
			v-else-if="failed"
			class="absolute inset-0 flex flex-col items-center justify-center gap-2 px-6 text-center text-sm text-ink-muted"
		>
			Could not load the graph
			<button
				type="button"
				class="rounded-lg border border-line bg-surface px-3 py-1 text-xs text-ink hover:bg-surface-hover"
				@click="reload"
			>
				Retry
			</button>
		</div>
		<div
			v-else-if="loading && !shown"
			class="absolute inset-0 flex items-center justify-center text-sm text-ink-muted"
		>
			Loading graph…
		</div>
	</div>
</template>

<script lang="ts">
	import { workspaceIdOf } from '@/components/graph/entity';
	import type { GraphResult } from '@/types/graph';
	import { Maximize2, Minus, Plus, Scan } from 'lucide-vue-next';
	import {
		computed,
		defineComponent,
		nextTick,
		onBeforeUnmount,
		onMounted,
		ref,
		watch,
		type PropType,
	} from 'vue';
	import GraphCanvas from './GraphCanvas.vue';
	import {
		ALL_GROUP_KEYS,
		filterGraph,
		includeFor,
		LEGEND_TYPES,
		TYPE_STYLES,
		type GraphGroupKey,
	} from './graphLogic';
	import { useDocumentTheme } from './useDocumentTheme';
	import { useGraphResult } from './useGraphResult';

	export default defineComponent({
		name: 'GraphNeighborhood',
		components: { GraphCanvas, Maximize2, Minus, Plus, Scan },
		props: {
			entity: { type: String, required: true },
			mode: { type: String as PropType<'mini' | 'full'>, default: 'mini' },
			depth: { type: Number as PropType<1 | 2>, default: 1 },
			groups: {
				type: Array as PropType<GraphGroupKey[]>,
				default: () => ALL_GROUP_KEYS,
			},
			selectedId: { type: String as PropType<string | null>, default: null },
			workspaceId: {
				type: [Number, String] as PropType<number | string | null>,
				default: null,
			},
		},
		emits: ['select', 'center', 'expand', 'loaded'],
		setup(props, { emit }) {
			const canvas = ref<{
				fit: () => void;
				zoomBy: (factor: number) => void;
			} | null>(null);
			const root = ref<HTMLElement | null>(null);
			const narrow = ref(false);
			const legendEl = ref<HTMLElement | null>(null);
			const inset = ref(0);
			const measure = () => {
				inset.value =
					narrow.value || !legendEl.value
						? 0
						: legendEl.value.offsetHeight + 20;
			};
			let observer: ResizeObserver | null = null;
			onMounted(() => {
				if (!root.value || typeof ResizeObserver === 'undefined') return;
				observer = new ResizeObserver(([entry]) => {
					narrow.value = entry.contentRect.width < 330;
					nextTick(measure);
				});
				observer.observe(root.value);
			});
			onBeforeUnmount(() => observer?.disconnect());
			const { dark } = useDocumentTheme();
			const { result, loading, failed, reload } = useGraphResult(() => ({
				entity: props.entity,
				depth: props.depth,
				include: includeFor(props.groups),
				workspace_id: props.workspaceId ?? workspaceIdOf(),
			}));

			const shown = computed<GraphResult | null>(() =>
				result.value ? filterGraph(result.value, props.groups) : null,
			);
			const empty = computed(
				() => !!shown.value && shown.value.nodes.length <= 1,
			);
			const legend = computed(() => {
				if (props.mode === 'full') return LEGEND_TYPES;
				const present = new Set(shown.value?.nodes.map((n) => n.type));
				return LEGEND_TYPES.filter((t) => present.has(t));
			});
			const truncatedNotice = computed(() => {
				const r = result.value;
				if (!r?.truncated) return '';
				const hidden = Object.values(r.caps_hit ?? {}).reduce(
					(sum, n) => sum + n,
					0,
				);
				const closest = r.nodes.length;
				return hidden
					? `Showing the closest ${closest} — ${hidden} more hidden`
					: `Showing the closest ${closest}`;
			});

			watch([legend, narrow], () => nextTick(measure));

			watch(shown, (value) => {
				if (value) emit('loaded', value);
			});

			const glyphStyle = (type: keyof typeof TYPE_STYLES) => {
				const style = TYPE_STYLES[type];
				const color = dark.value ? style.dark : style.light;
				if (style.shape === 'ring')
					return { border: `2px solid ${color}`, borderRadius: '50%' };
				if (style.shape === 'square')
					return { background: color, borderRadius: '3px' };
				if (style.shape === 'diamond')
					return {
						background: color,
						transform: 'rotate(45deg)',
						width: '9px',
						height: '9px',
					};
				return { background: color, borderRadius: '50%' };
			};

			return {
				canvas,
				root,
				narrow,
				legendEl,
				inset,
				shown,
				empty,
				legend,
				loading,
				failed,
				reload,
				truncatedNotice,
				glyphStyle,
				styles: TYPE_STYLES,
				fit: () => canvas.value?.fit(),
				zoom: (factor: number) => canvas.value?.zoomBy(factor),
			};
		},
	});
</script>

<style scoped>
	.legend-glyph {
		display: inline-block;
		width: 10px;
		height: 10px;
		box-sizing: border-box;
	}
</style>
