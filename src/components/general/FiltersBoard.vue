<template>
	<div class="w-full pl-4">
		<div
			class="w-full min-h-15 flex items-center justify-between mr-20 rounded"
		>
			<div class="flex items-center">
				<input
					class="cursor-pointer w-4 h-4 focus:outline-none rounded-lg"
					type="checkbox"
					id="checkbox"
					@change="$emit('handleUpdateDraggable', $event.target.checked)"
				/>
				<label class="ml-2 text-sm" for="checkbox">Reorder statuses</label>
			</div>
			<div class="flex">
				<div v-if="workspaceUsers.length >= 2" class="w-48 py-3">
					<Select
						placeholder="Select User"
						:options="workspaceUsers"
						v-model="selectedUser"
						label-key="name"
						value-key="id"
					/>
				</div>
			</div>
		</div>
	</div>
</template>

<script setup lang="ts">
	import { ref, watch } from 'vue';
	import { useStore } from 'vuex';
	import { defineEmits } from 'vue';
	import Select from 'src/components/general/Select.vue';
	const emit = defineEmits(['update:chosenUser', 'handleUpdateDraggable']);
	export interface UserOption {
		id: number;
		name: string;
		value: number;
		label: string;
	}
	interface Props {
		workspaceUsers: UserOption[];
		chosenUser: object;
		activeDraggable: boolean;
	}
	const props = defineProps<Props>();

	const store = useStore();
	const selectedUser = ref(0);
	const userOptions = props.workspaceUsers;
	const tasks = ref([]);

	watch(selectedUser, () => {
		emit(
			'update:chosenUser',
			userOptions.find((option) => option.id === selectedUser.value),
		);
	});
</script>
