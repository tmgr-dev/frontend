<template>
	<div class="flex flex-col gap-4" data-testid="map-insights">
		<div
			class="flex flex-col gap-3 rounded-2xl border border-line bg-surface p-4"
		>
			<div v-if="!bare" class="text-xs tracking-wide text-ink-muted">
				WHAT THE MAP SHOWS
			</div>
			<template v-if="insights">
				<button
					v-if="insights.bottleneck"
					type="button"
					class="insight text-left"
					data-testid="insight-bottleneck"
					@click="$emit('focus', insights.bottleneck.ids)"
				>
					<span
						class="text-sm font-semibold text-orange-600 dark:text-orange-300"
						>Bottleneck</span
					>
					<span class="text-sm leading-5">{{ insights.bottleneck.text }}</span>
				</button>
				<button
					v-if="insights.hubs"
					type="button"
					class="insight text-left"
					data-testid="insight-hubs"
					@click="$emit('focus', insights.hubs.ids)"
				>
					<span class="text-sm font-semibold">Hubs</span>
					<span class="text-sm leading-5">{{ insights.hubs.text }}</span>
				</button>
				<button
					v-if="insights.bridges"
					type="button"
					class="insight text-left"
					data-testid="insight-bridges"
					@click="$emit('focus', insights.bridges.ids)"
				>
					<span class="text-sm font-semibold">Bridges</span>
					<span class="text-sm leading-5">{{ insights.bridges.text }}</span>
				</button>
				<button
					type="button"
					class="insight text-left"
					data-testid="insight-orphans"
					@click="$emit('focus-orphans', insights.orphans.ids)"
				>
					<span class="text-sm font-semibold text-ink-muted">No links</span>
					<span class="text-sm leading-5">{{ insights.orphans.text }}</span>
				</button>
			</template>
			<p v-else class="text-sm text-ink-muted">
				Insights appear when the map has data.
			</p>
			<button
				type="button"
				class="rounded-lg bg-ink px-3.5 py-3 text-sm font-semibold text-surface-base"
				:aria-pressed="showOrphans"
				data-testid="toggle-orphans"
				@click="$emit('toggle-orphans')"
			>
				{{ showOrphans ? 'Hide' : 'Show' }} items with no links
			</button>
		</div>
		<div
			class="flex flex-col gap-2.5 rounded-2xl border border-line bg-surface p-4"
		>
			<div class="text-xs tracking-wide text-ink-muted">HOW TO USE IT</div>
			<p class="text-sm leading-5">
				Click a cluster to zoom in. Click a dot to open its neighborhood. Drag
				the time range to watch the map grow week by week.
			</p>
			<p v-if="legend" class="text-xs leading-4 text-ink-muted">
				{{ LEGEND }}
			</p>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import { MAP_LEGEND as LEGEND, type MapInsights } from './mapLogic';

	export default defineComponent({
		name: 'MapInsightsPanel',
		props: {
			insights: {
				type: Object as PropType<MapInsights | null>,
				default: null,
			},
			showOrphans: { type: Boolean, default: true },
			legend: { type: Boolean, default: false },
			bare: { type: Boolean, default: false },
		},
		emits: ['focus', 'focus-orphans', 'toggle-orphans'],
		setup() {
			return { LEGEND };
		},
	});
</script>

<style scoped>
	.insight {
		display: flex;
		flex-direction: column;
		gap: 4px;
		border-radius: 8px;
		margin: -4px -6px;
		padding: 4px 6px;
	}
	.insight:hover {
		background: var(--bg-hover);
	}
</style>
