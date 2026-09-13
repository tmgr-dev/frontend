<template>
	<AppTooltip :content="assignee.name" side="bottom">
		<div class="relative m-auto font-sans text-xs text-white">
			<UserAvatar
				v-if="assignee.has_avatar"
				:user-id="assignee.id"
				:name="assignee.name"
				has-avatar
				:size="24"
			/>
			<template v-else>{{ assignee.name.charAt(0).toUpperCase() }}</template>
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
	import AppTooltip from '@/components/general/AppTooltip.vue';
	import { Assignee } from '@/components/general/AssigneeUsers.vue';
	import UserAvatar from '@/components/general/UserAvatar.vue';

	interface Props {
		assignee: Assignee;
		showRemovingAssignee: Boolean;
	}

	defineProps<Props>();
	defineEmits<{
		deleteAssign: [id: number];
	}>();
</script>
