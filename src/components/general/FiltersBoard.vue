<template>
	<div class="w-full pl-4">
		<div
			class="mr-20 flex min-h-15 w-full items-center justify-between rounded"
		>
			<div class="flex">
				<div class="mr-3 flex items-center">
					<input
						class="h-4 w-4 cursor-pointer rounded-lg focus:outline-none"
						type="checkbox"
						id="checkbox"
						@change="$emit('handleUpdateDraggable', $event.target.checked)"
					/>
					<label class="ml-2 text-sm" for="checkbox">Reorder statuses</label>
				</div>

				<TextField placeholder="Search" v-model="searchText" />
			</div>

			<div class="flex">
				<div v-if="workspaceUsers.length >= 2" class="w-48 py-3">
					<Select
						placeholder="Select user"
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
	import { defineEmits } from 'vue';
	import Select from 'src/components/general/Select.vue';
	import TextField from 'src/components/general/TextField.vue';

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

	const emit = defineEmits([
		'update:chosenUser',
		'handleUpdateDraggable',
		'handleSearchTextChanged',
	]);
	const selectedUser = ref(0);
	const tasks = ref([]);
	const searchText = ref(null);

	watch(selectedUser, () => {
		emit(
			'update:chosenUser',
			props.workspaceUsers.find((option) => option.id === selectedUser.value),
		);
	});
	watch(searchText, (newValue) => {
		emit('handleSearchTextChanged', newValue);
	});
</script>
