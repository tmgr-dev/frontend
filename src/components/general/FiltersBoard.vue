<template>
	<div class="w-full pl-4">
		<div
			class="mr-20 flex flex-col md:flex-row min-h-15 w-full items-center justify-between rounded"
		>
			<div class="mr-3 flex items-center">
				<input
					class="h-4 w-4 cursor-pointer rounded-lg focus:outline-none"
					type="checkbox"
					id="checkbox"
					@change="$emit('handleUpdateDraggable', $event.target.checked)"
				/>
				<label class="ml-2 text-sm" for="checkbox">Reorder statuses</label>
			</div>
			<div class="flex flex-col m-2 md:flex-row">
				<TextField
					placeholder="Search"
					v-model="searchText"
					input-class="py-1 dark:border-transparent"
				/>

				<div>
					<div v-if="categories.length >= 2" class="md:m-0 md:ml-3 mt-2 w-48">
						<Select
							placeholder="Select category"
							:options="categories"
							v-model="selectedCategory"
							label-key="title"
							value-key="id"
						/>
					</div>
				</div>
			</div>

			<div>
				<div v-if="workspaceUsers.length >= 2" class="w-48">
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
	import { onMounted, Ref, ref, watch } from 'vue';
	import { defineEmits } from 'vue';
	import Select from 'src/components/general/Select.vue';
	import TextField from 'src/components/general/TextField.vue';

	export interface UserOption {
		id: number;
		name: string;
		value: number;
		label: string;
	}
	export interface CategoryOption {
		id: number;
		title: string;
		value: number;
		label: string;
	}
	interface Props {
		workspaceUsers: UserOption[];
		chosenUser: object;
		activeDraggable: boolean;
		categories: CategoryOption[];
	}
	const props = defineProps<Props>();

	const emit = defineEmits([
		'update:chosenUser',
		'handleUpdateDraggable',
		'handleSearchTextChanged',
		'handleChosenCategory',
	]);
	const selectedUser = ref(0);
	const tasks = ref([]);
	const searchText = ref(null);
	const selectedCategory = ref(0);

	watch(selectedUser, () => {
		emit(
			'update:chosenUser',
			props.workspaceUsers.find((option) => option.id === selectedUser.value),
		);
	});
	watch(selectedCategory, () => {
		emit(
			'handleChosenCategory',
			props.categories.find((option) => option.id === selectedCategory.value),
		);
	});
	watch(searchText, (newValue) => {
		emit('handleSearchTextChanged', newValue);
	});
</script>
