<template>
	<AppTooltip :content="assignee.name" side="bottom">
		<div class="relative m-auto font-sans text-xs text-white">
			{{ assignee.name.charAt(0).toUpperCase() }}
		</div>
	</AppTooltip>
	<div
		v-if="showRemovingAssignee"
		class="invisible absolute -right-1.5 -top-1.5 flex h-4 w-4 cursor-pointer rounded-full bg-red-500 opacity-75 hover:opacity-100 group-hover:visible"
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
	import { Assignee } from '@/components/general/AssigneeUsers.vue';
	import AppTooltip from '@/components/general/AppTooltip.vue';

	interface Props {
		assignee: Assignee;
		showRemovingAssignee: Boolean;
	}

	defineProps<Props>();
	defineEmits<{
		deleteAssign: [id: number];
	}>();
</script>
