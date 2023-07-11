<template>
	<div class="w-full pl-4">
		<div
			class="mr-20 flex min-h-[62px] w-full flex-col items-center justify-between rounded md:flex-row"
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
			<div class="m-2 flex flex-col md:flex-row">
				<div class="mr-2 hidden items-center justify-center md:flex">
					<span
						class="material-icons cursor-pointer duration-300 ease-in-out hover:scale-95 hover:text-blue-200"
						@click="loadTasks"
					>
						refresh
					</span>
				</div>
				<TextField
					placeholder="Search"
					v-model="searchText"
					input-class="py-1 w-48 dark:border-transparent"
				/>

				<div>
					<div
						v-if="categories.length >= 2"
						class="mt-2 w-48 md:m-0 md:ml-3 md:mr-3"
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
				<div v-if="workspaceUsers.length >= 2" class="mt-2 w-48 md:mt-0">
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
	import { computed, onMounted, watch } from 'vue';
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

	import { useStore } from 'vuex';
	import { useRouter } from 'vue-router';

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
		'loadTasks',
		'loadColumns',
	]);

	const store = useStore();
	const router = useRouter();
	const currentRoute = router.currentRoute.value;

	const selectedCategory = computed({
		get: () => (store.state as { filter: State }).filter.selectedCategory,
		set: (value) => {
			store.commit('updateSelectedCategory', value);
		},
	});

	const searchText = computed({
		get: () => (store.state as { filter: State }).filter.searchText,
		set: (value) => {
			store.commit('updateSearchText', value);
		},
	});

	const selectedUser = computed({
		get: () => (store.state as { filter: State }).filter.selectedUser,
		set: (value) => {
			store.commit('updateSelectedUser', value);
		},
	});

	const buildQuery = () => {
		const query: Record<string, string> = {};

		if (searchText.value) {
			query.search = searchText.value;
		}

		if (selectedCategory.value) {
			query.category = selectedCategory.value.toString();
		}

		if (selectedUser.value) {
			query.user = selectedUser.value.toString();
		}

		return query;
	};

	onMounted(() => {
		const query = currentRoute.query;

		if (query.search) {
			searchText.value = query.search as string;
		}

		if (query.category) {
			selectedCategory.value = Number(query.category);
		}

		if (query.user) {
			selectedUser.value = Number(query.user);
		}
	});

	watch(selectedUser, () => {
		emit(
			'update:chosenUser',
			props.workspaceUsers.find((option) => option.id === selectedUser.value),
		);
		router.push({
			...currentRoute.query,
			query: buildQuery(),
		});
	});

	watch(selectedCategory, () => {
		emit(
			'handleChosenCategory',
			props.categories.find((option) => option.id === selectedCategory.value),
		);
		router.push({
			...currentRoute.query,
			query: buildQuery(),
		});
	});

	watch(searchText, (newValue) => {
		emit('handleSearchTextChanged', newValue);

		router.push({
			...currentRoute.query,
			query: buildQuery(),
		});
	});
	const loadTasks = () => {
		emit('loadColumns');
		emit('loadTasks');
	};
</script>
