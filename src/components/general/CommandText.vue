<template>
	<span>
		<template v-for="(part, index) in parts" :key="index">
			<CommandChip v-if="part.kind === 'command'" :command="part.value" />
			<template v-else>{{ part.value }}</template>
		</template>
	</span>
</template>

<script lang="ts">
	import CommandChip from '@/components/general/CommandChip.vue';
	import { splitCommandText } from '@/utils/commandText';
	import { computed, defineComponent } from 'vue';

	export default defineComponent({
		name: 'CommandText',
		components: { CommandChip },
		props: {
			text: { type: String, required: true },
		},
		setup(props) {
			const parts = computed(() => splitCommandText(props.text));
			return { parts };
		},
	});
</script>
