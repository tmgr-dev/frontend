<template>
	<div
		class="flex items-center justify-center gap-2 text-sm text-ink"
		role="status"
		aria-live="polite"
		data-tour="connect.status"
	>
		<span class="h-2.5 w-2.5 rounded-full" :class="dot" aria-hidden="true" />
		<span>{{ text }}</span>
	</div>
</template>

<script lang="ts">
	import type { PropType } from 'vue';
	import { computed, defineComponent } from 'vue';

	export type ConnectState = 'waiting' | 'connected' | 'stalled';

	export default defineComponent({
		name: 'ConnectStatus',
		props: {
			state: { type: String as PropType<ConnectState>, required: true },
		},
		setup(props) {
			const text = computed(() =>
				props.state === 'connected'
					? 'Connected'
					: props.state === 'stalled'
					? 'Not connected yet'
					: 'Waiting for your agent…',
			);
			const dot = computed(() =>
				props.state === 'connected'
					? 'bg-green-500'
					: props.state === 'stalled'
					? 'bg-amber-500'
					: 'animate-pulse bg-red-400',
			);
			return { text, dot };
		},
	});
</script>
