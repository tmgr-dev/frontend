<template>
	<span
		:class="[
			'inline-flex h-[22px] items-center gap-1.5 whitespace-nowrap rounded-pill px-2 text-2xs font-semibold',
			TONE_CLASS[view.tone],
			view.highlight && 'agent-attention',
		]"
		:data-agent-state="state.kind"
		:data-attention="view.highlight ? 'true' : undefined"
	>
		<span
			v-if="view.tone === 'active' || view.highlight"
			class="h-1.5 w-1.5 rounded-full bg-current"
			aria-hidden="true"
		/>{{ view.label }}
	</span>
</template>

<script setup lang="ts">
	import { computed } from 'vue';
	import type { RunnerState } from '@/runner/api';
	import { describeState, TONE_CLASS } from '@/runner/states';

	const props = defineProps<{ state: RunnerState }>();
	const view = computed(() => describeState(props.state));
</script>

<style scoped>
	@media (prefers-reduced-motion: no-preference) {
		.agent-attention {
			animation: agent-attention-pulse 1.6s ease-in-out infinite;
		}
	}

	@keyframes agent-attention-pulse {
		50% {
			box-shadow: 0 0 0 4px rgb(245 158 11 / 0.25);
		}
	}
</style>
