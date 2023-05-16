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
					:checked="activeDraggable"
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
					<div
						v-if="categories.length >= 2"
						class="md:m-0 md:ml-3 md:mr-3 mt-2 w-48"
					>
						<Select
							placeholder="Select category"
							:options="categories"
							v-model="selectedCategory"
							label-key="title"
							value-key="id"
						/>
					</div>
				</div>
				<div v-if="workspaceUsers.length >= 2" class="w-48 mt-2 md:mt-0">
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
	import { computed, ref, watch } from 'vue';
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

	import { createStore, useStore } from 'vuex';

	interface State {
		selectedCategory: number;
		searchText: string | null;
		selectedUser: number;
	}
	const props = defineProps<Props>();

	const emit = defineEmits([
		'update:chosenUser',
		'handleUpdateDraggable',
		'handleSearchTextChanged',
		'handleChosenCategory',
	]);
	const store = useStore<State>();

	const selectedCategory = computed({
		get: () => store.state.selectedCategory,
		set: (value) => {
			store.commit('updateSelectedCategory', value);
		},
	});
	const searchText = computed({
		get: () => store.state.searchText,
		set: (value) => {
			store.commit('updateSearchText', value);
		},
	});
	const selectedUser = computed({
		get: () => store.state.selectedUser,
		set: (value) => {
			store.commit('updateSelectedUser', value);
		},
	});

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
