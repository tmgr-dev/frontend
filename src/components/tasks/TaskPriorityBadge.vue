<template>
	<span
		v-if="visible"
		class="inline-flex shrink-0 items-center"
		:title="`${option.label} priority`"
		data-testid="task-priority-badge"
		:data-priority="option.value"
	>
		<component
			:is="option.icon"
			class="h-3.5 w-3.5"
			:class="option.colorClass"
			aria-hidden="true"
		/>
		<span class="sr-only">{{ option.label }} priority</span>
	</span>
</template>

<script lang="ts">
	import { priorityOption, showPriorityBadge } from '@/utils/priority';
	import { computed, defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'TaskPriorityBadge',
		props: {
			priority: { type: String as PropType<string | null>, default: null },
		},
		setup(props) {
			return {
				visible: computed(() => showPriorityBadge(props.priority)),
				option: computed(() => priorityOption(props.priority)),
			};
		},
	});
</script>
