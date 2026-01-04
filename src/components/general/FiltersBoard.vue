<template>
	<div class="w-full xl-custom:pl-4">
		<div
			class="flex min-h-[62px] w-full flex-col items-stretch justify-between rounded xl-custom:mr-20 xl-custom:flex-row xl-custom:items-center"
		>
			<div class="mr-3 flex items-center">
			</div>
			<div class="flex flex-col gap-3 xl-custom:m-2 xl-custom:flex-row xl-custom:gap-0">
				<div v-if="!isMobileModal" class="flex items-center justify-between xl-custom:mr-2 xl-custom:hidden">
					<span
						class="material-icons cursor-pointer duration-300 ease-in-out hover:scale-95 hover:text-blue-200"
						@click="loadTasks"
					>
						refresh
					</span>
					<span
						v-if="hasActiveFilters"
						@click="clearFilters"
						class="material-icons cursor-pointer duration-300 ease-in-out hover:scale-95 hover:text-red-200"
						title="Clear all filters"
					>
						clear
					</span>
				</div>
				<div class="mr-2 hidden items-center justify-center xl-custom:flex">
					<span
						class="material-icons cursor-pointer duration-300 ease-in-out hover:scale-95 hover:text-blue-200"
						@click="loadTasks"
					>
						refresh
					</span>
					<span
						v-if="hasActiveFilters"
						@click="clearFilters"
						class="material-icons cursor-pointer duration-300 ease-in-out hover:scale-95 hover:text-red-200"
						title="Clear all filters"
					>
					clear
					</span>
				</div>
				<TextField
					v-if="isUserFeatureEnabled('board.search_input')"
					placeholder="Search"
					v-model="searchText"
					input-class="py-1 w-full xl-custom:w-48 dark:border-transparent"
				/>

				<div>
					<div
						v-if="isFeatureEnabled('categories') && isUserFeatureEnabled('board.category_filter') && categories.length >= 2"
						class="w-full xl-custom:m-0 xl-custom:ml-3 xl-custom:mr-3 xl-custom:w-48"
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
				<div v-if="isUserFeatureEnabled('board.user_filter') && workspaceUsers.length >= 2" class="w-full xl-custom:mt-0 xl-custom:w-48">
					<Select
						placeholder="Select user"
						:options="workspaceUsers"
						v-model="selectedUser"
						label-key="name"
						value-key="id"
					/>
				</div>
				
				<button
					v-if="isMobileModal && hasActiveFilters"
					type="button"
					@click="clearFilters"
					class="w-full flex items-center justify-center gap-2 px-4 py-2 text-sm text-gray-700 dark:text-gray-200 bg-gray-200 hover:bg-gray-300 dark:bg-gray-700 dark:hover:bg-gray-600 rounded-lg transition-colors xl-custom:hidden"
				>
					<span class="material-icons text-base">clear</span>
					<span>Clear filters</span>
				</button>
				
				<div v-if="isMobileModal" class="flex flex-col gap-3 border-t pt-3 mt-3">
					<button
						type="button"
						@click="handleMobileReorderClick"
						class="w-full px-4 py-2 text-sm text-left bg-gray-200 hover:bg-gray-300 dark:bg-gray-700 dark:hover:bg-gray-600 text-gray-700 dark:text-gray-200 rounded-lg transition-colors"
					>
						Reorder statuses
					</button>
					<slot name="actions-start"></slot>
				</div>
				
				<div v-else class="flex items-center justify-center xl-custom:ml-3 xl-custom:justify-start">
					<Dropdown>
						<MenuItem>
							<div class="flex items-center px-4 py-2 text-neutral-600 dark:text-gray-200">
								<input
									class="h-4 w-4 cursor-pointer rounded-lg focus:outline-none"
									type="checkbox"
									id="checkbox"
									@change="$emit('handleUpdateDraggable', $event.target.checked)"
									:checked="activeDraggable"
								/>
								<label class="ml-2 text-sm cursor-pointer" for="checkbox">Reorder statuses</label>
							</div>
						</MenuItem>
						<MenuItem>
							<slot name="actions-start"></slot>
						</MenuItem>
					</Dropdown>
				</div>
			</div>
		</div>
	</div>
</template>

<script setup lang="ts">
	import { computed, onMounted, watch } from 'vue';
	import Select from '@/components/general/Select.vue';
	import TextField from '@/components/general/TextField.vue';
	import Dropdown from '@/components/general/Dropdown.vue';
	import { MenuItem } from '@headlessui/vue';

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
		isMobileModal?: boolean;
	}

	import { useStore } from 'vuex';
	import { useRouter } from 'vue-router';
	import { useFeatureToggles } from '@/composable/useFeatureToggles';

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
		'close-modal',
		'open-reorder-modal',
	]);

	const handleMobileReorderClick = () => {
		emit('open-reorder-modal');
		emit('close-modal');
	};

	const store = useStore();
	const router = useRouter();
	const currentRoute = router.currentRoute.value;
	const { isFeatureEnabled, isUserFeatureEnabled } = useFeatureToggles();

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

	const hasActiveFilters = computed(() => {
		return !!(searchText.value || selectedCategory.value || selectedUser.value);
	});

	const clearFilters = () => {
		searchText.value = null;
		selectedCategory.value = 0;
		selectedUser.value = 0;
		
		router.push({
			...currentRoute,
			query: {},
		});
	};

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
