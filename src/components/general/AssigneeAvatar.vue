<template>
	<div class="m-auto font-sans text-base text-white">
		{{ assignee?.name.charAt(0).toUpperCase() }}
	</div>

	<div
		v-if="showRemovingAssignee"
		class="invisible absolute -top-1.5 -right-1.5 flex h-4 w-4 cursor-pointer rounded-full bg-red-500 opacity-75 hover:opacity-100 group-hover:visible"
	>
		<span
			class="material-icons m-auto text-xs text-white"
			@click="$emit('deleteAssign', assignee.id)"
		>
			close
		</span>
	</div>
</template>

<script setup lang="ts">
	import type { PropType } from 'vue';
	interface Assignee {
		name: string;
		id: number;
	}
	interface Props {
		assignee: Assignee;
		showRemovingAssignee: Boolean;
	}
	const props = defineProps({
		showRemovingAssignee: {
			type: Boolean,
			default: true,
		},
		assignee: {
			type: Object as PropType<Assignee>,
			required: true,
		},
	});
</script>
