<template>
	<div class="w-full min-w-0">
		<div
			ref="root"
			role="img"
			tabindex="0"
			aria-roledescription="chart"
			:aria-label="summary"
			class="relative w-full rounded-md outline-none focus-visible:ring-2 focus-visible:ring-sky-500 dark:focus-visible:ring-sky-400"
			@pointermove="handlePointer"
			@pointerleave="active = null"
			@blur="active = null"
			@keydown="handleKeydown"
		>
			<svg
				:viewBox="`0 0 ${width} ${node.height}`"
				width="100%"
				:height="node.height"
				aria-hidden="true"
				class="block select-none"
			>
				<g v-memo="[model]">
					<g
						v-for="tick in model.ticks"
						:key="tick.value"
						class="text-gray-200 dark:text-gray-700"
					>
						<line
							:x1="model.box.left"
							:x2="model.box.left + model.box.width"
							:y1="tick.y"
							:y2="tick.y"
							stroke="currentColor"
							stroke-width="1"
						/>
						<text
							:x="model.box.left - 6"
							:y="tick.y"
							text-anchor="end"
							dominant-baseline="middle"
							font-size="10"
							class="fill-gray-500 dark:fill-gray-400"
						>
							{{ tick.text }}
						</text>
					</g>
					<text
						v-for="item in model.xLabels"
						:key="item.index"
						:x="item.x"
						:y="model.box.top + model.box.height + 16"
						text-anchor="middle"
						font-size="10"
						class="fill-gray-500 dark:fill-gray-400"
					>
						{{ item.text }}
					</text>
					<rect
						v-for="bar in model.bars"
						:key="`${bar.series}-${bar.index}`"
						:x="bar.x"
						:y="bar.y"
						:width="bar.width"
						:height="bar.height"
						fill="currentColor"
						:class="model.classes[bar.series]"
					/>
					<path
						v-for="(line, i) in model.lines"
						:key="i"
						:d="line"
						fill="none"
						stroke="currentColor"
						stroke-width="2"
						stroke-linecap="round"
						stroke-linejoin="round"
						:class="model.classes[i]"
					/>
				</g>
				<template v-if="active !== null">
					<line
						:x1="activeX"
						:x2="activeX"
						:y1="model.box.top"
						:y2="model.box.top + model.box.height"
						stroke="currentColor"
						stroke-width="1"
						class="text-gray-400 dark:text-gray-500"
					/>
					<template v-if="node.kind === 'line'">
						<circle
							v-for="(s, i) in node.series"
							v-show="s.values[active] != null"
							:key="i"
							:cx="activeX"
							:cy="activeY(i)"
							r="3.5"
							fill="currentColor"
							:class="model.classes[i]"
						/>
					</template>
				</template>
			</svg>
			<div
				v-if="active !== null"
				:class="[
					'pointer-events-none absolute top-2 z-10 max-w-[60%] min-w-28 rounded-md border border-gray-200 bg-white px-2 py-1.5 text-xs text-gray-900 shadow-md dark:border-gray-700 dark:bg-gray-900 dark:text-gray-100',
				]"
				:style="tooltipStyle"
			>
				<div class="mb-1 font-medium break-words">
					{{ node.labels[active] }}
				</div>
				<div
					v-for="(s, i) in node.series"
					:key="i"
					class="flex items-center justify-between gap-3"
				>
					<span class="flex min-w-0 items-center gap-1.5">
						<span
							:class="[
								'h-2 w-2 shrink-0 rounded-full bg-current',
								model.classes[i],
							]"
						/>
						<span class="truncate">{{ s.name }}</span>
					</span>
					<span class="tabular-nums">{{ formatValue(s.values[active]) }}</span>
				</div>
			</div>
		</div>
		<div
			v-if="node.series.length > 1"
			class="mt-1 flex flex-wrap gap-x-3 gap-y-1 text-xs text-gray-600 dark:text-gray-300"
		>
			<span
				v-for="(s, i) in node.series"
				:key="i"
				class="flex min-w-0 items-center gap-1.5"
			>
				<span
					:class="[
						'h-2 w-2 shrink-0 rounded-full bg-current',
						model.classes[i],
					]"
				/>
				<span class="truncate">{{ s.name }}</span>
			</span>
		</div>
		<div class="sr-only" aria-live="polite">{{ liveText }}</div>
	</div>
</template>

<script lang="ts">
	import type { UiNode } from '@/pluginSystem/uiTree';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
		watch,
		type PropType,
	} from 'vue';
	import {
		CHART_DEFAULT_WIDTH,
		CHART_STROKE_CLASS,
		barRects,
		chartScale,
		chartSummary,
		clampActive,
		formatTick,
		formatValue,
		linePath,
		nearestIndex,
		plotBox,
		shortAxisLabel,
		stepIndex,
		visibleLabelIndexes,
		xCenter,
		yOf,
	} from './pluginChart';

	type ChartNode = Extract<UiNode, { type: 'chart' }>;

	export default defineComponent({
		name: 'PluginChart',
		props: {
			node: { type: Object as PropType<ChartNode>, required: true },
		},
		setup(props) {
			const root = ref<HTMLElement | null>(null);
			const width = ref(CHART_DEFAULT_WIDTH);
			const active = ref<number | null>(null);
			let observer: ResizeObserver | null = null;

			const measure = () => {
				const w = Math.round(root.value?.clientWidth ?? 0);
				if (w >= 80) width.value = w;
			};
			onMounted(() => {
				measure();
				if (typeof ResizeObserver !== 'undefined' && root.value) {
					observer = new ResizeObserver(measure);
					observer.observe(root.value);
				}
			});
			onBeforeUnmount(() => observer?.disconnect());

			const model = computed(() => {
				const node = props.node;
				const rows = node.series.map((s) => s.values);
				const scale = chartScale(rows, node.kind === 'bar');
				const box = plotBox(width.value, node.height, scale);
				const count = node.labels.length;
				return {
					scale,
					box,
					count,
					ticks: scale.ticks.map((value) => ({
						value,
						text: formatTick(value),
						y: Math.round(yOf(value, scale, box) * 100) / 100,
					})),
					xLabels: visibleLabelIndexes(node.labels, box).map((index) => ({
						index,
						text: shortAxisLabel(node.labels[index]),
						x: xCenter(index, count, box),
					})),
					bars: node.kind === 'bar' ? barRects(rows, scale, box) : [],
					lines:
						node.kind === 'line'
							? rows.map((row) => linePath(row, scale, box))
							: [],
					classes: node.series.map((s) => CHART_STROKE_CLASS[s.color]),
				};
			});

			watch(
				() => props.node.labels.length,
				(count) => {
					active.value = clampActive(active.value, count);
				},
			);

			const activeX = computed(() =>
				active.value === null
					? 0
					: xCenter(active.value, model.value.count, model.value.box),
			);
			const activeY = (series: number) => {
				const v = props.node.series[series].values[active.value ?? 0];
				return v == null ? 0 : yOf(v, model.value.scale, model.value.box);
			};
			const tooltipStyle = computed(() => {
				const pct = (activeX.value / width.value) * 100;
				return pct > 55
					? { right: `${100 - pct + 1}%` }
					: { left: `${pct + 1}%` };
			});
			const summary = computed(() =>
				chartSummary(
					props.node.kind,
					props.node.series.map((s) => s.name),
					props.node.labels.length,
					props.node.label,
				),
			);
			const liveText = computed(() =>
				active.value === null
					? ''
					: `${props.node.labels[active.value]}: ${props.node.series
							.map((s) => `${s.name} ${formatValue(s.values[active.value!])}`)
							.join(', ')}`,
			);

			const handlePointer = (event: PointerEvent) => {
				const svg = (event.currentTarget as HTMLElement).querySelector('svg');
				if (!svg) return;
				const rect = svg.getBoundingClientRect();
				active.value = nearestIndex(
					event.clientX,
					rect.left,
					rect.width,
					width.value,
					model.value.box,
					model.value.count,
				);
			};
			const handleKeydown = (event: KeyboardEvent) => {
				const count = model.value.count;
				if (event.key === 'Escape') {
					if (active.value === null) return;
					event.stopPropagation();
					active.value = null;
					return;
				}
				const next =
					event.key === 'ArrowRight'
						? stepIndex(active.value, 1, count)
						: event.key === 'ArrowLeft'
							? stepIndex(active.value, -1, count)
							: event.key === 'Home'
								? 0
								: event.key === 'End'
									? count - 1
									: undefined;
				if (next === undefined) return;
				event.preventDefault();
				active.value = next;
			};

			return {
				root,
				width,
				active,
				model,
				activeX,
				activeY,
				tooltipStyle,
				summary,
				liveText,
				formatValue,
				handlePointer,
				handleKeydown,
			};
		},
	});
</script>
