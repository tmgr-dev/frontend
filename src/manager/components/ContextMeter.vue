<template>
	<span :class="['ctx', 'mini', level]"
		><span class="bar"><i :style="{ width: `${pct ?? 0}%` }"></i></span
		><em>{{ text }}</em></span
	>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';
	import { contextFull, contextLevel, formatContext } from './viewHelpers';

	export default defineComponent({
		name: 'ManagerContextMeter',
		props: {
			tokens: { type: Number, default: undefined },
			pct: { type: Number, default: undefined },
			full: { type: Boolean, default: false },
		},
		computed: {
			level(): string {
				return contextLevel(this.pct);
			},
			text(): string {
				return this.full
					? contextFull(this.tokens, this.pct)
					: formatContext(this.tokens);
			},
		},
	});
</script>
