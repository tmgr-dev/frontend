<template>
	<div
		class="bg-surface/90 pointer-events-auto flex items-center gap-3 rounded-full border border-line py-1.5 pl-1.5 pr-4 shadow-sm backdrop-blur"
		data-testid="map-timeline"
	>
		<button
			type="button"
			class="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-ink text-surface-base"
			:aria-label="playing ? 'Pause timeline' : 'Play timeline'"
			data-testid="map-play"
			@click="$emit('toggle')"
		>
			<Pause v-if="playing" class="h-4 w-4" />
			<Play v-else class="h-4 w-4" />
		</button>
		<input
			type="range"
			class="map-range min-w-0 flex-1"
			min="0"
			:max="Math.max(0, steps - 1)"
			step="1"
			:value="modelValue"
			aria-label="Timeline"
			data-testid="map-scrubber"
			@input="
				$emit(
					'update:modelValue',
					Number(($event.target as HTMLInputElement).value),
				)
			"
		/>
		<span
			class="w-14 shrink-0 text-right text-xs tabular-nums text-ink-muted"
			data-testid="map-cursor-label"
			>{{ label }}</span
		>
	</div>
</template>

<script lang="ts">
	import { Pause, Play } from 'lucide-vue-next';
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'MapTimeline',
		components: { Pause, Play },
		props: {
			modelValue: { type: Number, required: true },
			steps: { type: Number, required: true },
			label: { type: String, required: true },
			playing: { type: Boolean, default: false },
		},
		emits: ['update:modelValue', 'toggle'],
	});
</script>

<style scoped>
	.map-range {
		appearance: none;
		height: 4px;
		border-radius: 999px;
		background: var(--line-strong-color);
		outline: none;
	}
	.map-range::-webkit-slider-thumb {
		appearance: none;
		width: 14px;
		height: 14px;
		border-radius: 999px;
		background: var(--fg);
		border: 2px solid var(--bg-raised);
		cursor: grab;
	}
	.map-range::-moz-range-thumb {
		width: 12px;
		height: 12px;
		border-radius: 999px;
		background: var(--fg);
		border: 2px solid var(--bg-raised);
		cursor: grab;
	}
</style>
