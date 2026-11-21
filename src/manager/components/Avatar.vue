<template>
	<span
		:class="['av', size, { mgr: manager }]"
		:style="{ '--ac': agentColor(agent) }"
		aria-hidden="true"
		>{{ avatarLetter(agent, manager) }}</span
	>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';
	import { agentColor, avatarLetter } from './viewHelpers';

	export default defineComponent({
		name: 'ManagerAvatar',
		props: {
			agent: { type: String, default: undefined },
			manager: { type: Boolean, default: false },
			size: { type: String, default: '' },
		},
		methods: { agentColor, avatarLetter },
	});
</script>
