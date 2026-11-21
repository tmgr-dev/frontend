<template>
	<svg
		viewBox="0 0 24 24"
		width="1em"
		height="1em"
		:fill="icon.fill ? 'currentColor' : 'none'"
		:stroke="icon.fill ? 'none' : 'currentColor'"
		:stroke-width="icon.sw ?? 2"
		stroke-linecap="round"
		stroke-linejoin="round"
		aria-hidden="true"
		v-html="icon.body"
	></svg>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';
	import { ICONS } from './viewHelpers';

	export default defineComponent({
		name: 'ManagerIcon',
		props: { name: { type: String, required: true } },
		computed: {
			icon() {
				return ICONS[this.name] ?? ICONS.dot;
			},
		},
	});
</script>
