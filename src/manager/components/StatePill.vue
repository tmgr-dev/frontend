<template>
	<span :class="['pill', `st-${state}`]"
		><Icon :name="iconName" />{{ label }}</span
	>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';
	import Icon from './Icon.vue';

	const ICON_BY_STATE: Record<string, string> = {
		working: 'bolt',
		waiting: 'hand',
		failed: 'x',
		idle: 'dot',
		done: 'check',
	};
	const KNOWN = ['working', 'waiting', 'failed', 'idle', 'done'];

	export default defineComponent({
		name: 'ManagerStatePill',
		components: { Icon },
		props: { state: { type: String, required: true } },
		computed: {
			iconName(): string {
				return ICON_BY_STATE[this.state] ?? 'dot';
			},
			label(): string {
				return KNOWN.includes(this.state)
					? this.$t(`manager.state.${this.state}`)
					: this.state;
			},
		},
	});
</script>
