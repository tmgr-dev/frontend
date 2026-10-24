<template>
	<span
		:class="[
			'inline-flex h-[22px] items-center gap-1.5 rounded-pill px-2 text-2xs font-semibold whitespace-nowrap',
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

<script lang="ts">
	import type { RunnerState } from '@/runner/api';
	import { describeState, TONE_CLASS } from '@/runner/states';
	import { computed, defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'AgentStateChip',
		props: {
			state: { type: Object as PropType<RunnerState>, required: true },
		},
		setup(props) {
			const view = computed(() => describeState(props.state));
			return { view, TONE_CLASS };
		},
	});
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
