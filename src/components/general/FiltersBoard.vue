<template>
	<!-- Mobile/modal layout (kept simple — old TextField/Select work fine inside the modal) -->
	<div v-if="isMobileModal" class="w-full">
		<div class="flex flex-col gap-3 rounded">
			<TextField
				v-if="isUserFeatureEnabled('board.search_input')"
				placeholder="Search"
				v-model="searchText"
				input-class="py-1 w-full"
			/>

			<div
				v-if="isFeatureEnabled('categories') && isUserFeatureEnabled('board.category_filter') && categories.length >= 2"
				class="w-full"
			>
				<Select
					placeholder="Select category"
					:options="categories"
					v-model="selectedCategory"
					label-key="title"
					value-key="id"
				/>
			</div>

			<div v-if="isUserFeatureEnabled('board.user_filter') && workspaceUsers.length >= 2" class="w-full">
				<Select
					placeholder="Select user"
					:options="workspaceUsers"
					v-model="selectedUser"
					label-key="name"
					value-key="id"
				/>
			</div>

			<button
				v-if="hasActiveFilters"
				type="button"
				@click="clearFilters"
				class="w-full flex items-center justify-center gap-2 px-4 py-2 text-sm rounded-md border border-line bg-surface text-ink hover:bg-surface-hover transition-colors"
			>
				<span class="material-icons text-base">clear</span>
				<span>Clear filters</span>
			</button>

			<div class="flex flex-col gap-3 border-t border-line pt-3 mt-3">
				<button
					type="button"
					@click="handleMobileReorderClick"
					class="w-full px-4 py-2 text-sm text-left rounded-md border border-line bg-surface text-ink hover:bg-surface-hover transition-colors"
				>
					Reorder statuses
				</button>
				<slot name="actions-start"></slot>
			</div>
		</div>
	</div>

	<!-- Desktop pill-style filters (header layout) -->
	<div v-else class="flex items-center gap-2 min-w-0">
		<!-- Search -->
		<div
			v-if="isUserFeatureEnabled('board.search_input')"
			class="relative min-w-0"
		>
			<MagnifyingGlassIcon class="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink-subtle" />
			<input
				type="text"
				v-model="searchText"
				placeholder="Search tasks…"
				class="h-9 w-44 lg:w-56 rounded-pill border border-line bg-surface text-ink pl-9 pr-3 text-sm placeholder:text-ink-subtle outline-none focus:border-line-strong"
			/>
		</div>

		<!-- Category select -->
		<div
			v-if="isFeatureEnabled('categories') && isUserFeatureEnabled('board.category_filter') && categories.length >= 2"
			class="relative shrink-0"
		>
			<select
				v-model="selectedCategory"
				class="h-9 w-40 lg:w-48 appearance-none rounded-pill border border-line bg-surface text-ink pl-3 pr-9 text-sm outline-none focus:border-line-strong"
			>
				<option :value="0">All categories</option>
				<option v-for="cat in categories" :key="cat.id" :value="cat.id">
					{{ cat.title }}
				</option>
			</select>
			<ChevronDownIcon class="pointer-events-none absolute right-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink-subtle" />
		</div>

		<!-- User select -->
		<div
			v-if="isUserFeatureEnabled('board.user_filter') && workspaceUsers.length >= 2"
			class="relative shrink-0"
		>
			<select
				v-model="selectedUser"
				class="h-9 w-32 lg:w-40 appearance-none rounded-pill border border-line bg-surface text-ink pl-3 pr-9 text-sm outline-none focus:border-line-strong"
			>
				<option :value="0">All users</option>
				<option v-for="u in workspaceUsers" :key="u.id" :value="u.id">
					{{ u.name }}
				</option>
			</select>
			<ChevronDownIcon class="pointer-events-none absolute right-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink-subtle" />
		</div>

		<!-- Separator -->
		<div class="h-5 w-px bg-line shrink-0"></div>

		<!-- Refresh -->
		<button
			type="button"
			@click="loadTasks"
			class="flex h-9 w-9 shrink-0 items-center justify-center rounded-pill text-ink-subtle hover:bg-surface-hover hover:text-ink"
			title="Refresh"
		>
			<ArrowPathIcon class="h-4 w-4" />
		</button>

		<!-- Clear active filters -->
		<button
			v-if="hasActiveFilters"
			type="button"
			@click="clearFilters"
			class="flex h-9 w-9 shrink-0 items-center justify-center rounded-pill text-ink-subtle hover:bg-status-fix-bg hover:text-status-fix-fg"
			title="Clear filters"
		>
			<XMarkIcon class="h-4 w-4" />
		</button>

		<!-- Actions dropdown -->
		<Dropdown>
			<MenuItem>
				<div class="flex items-center px-3 py-2 text-sm text-ink">
					<input
						class="h-4 w-4 cursor-pointer rounded focus:outline-none"
						type="checkbox"
						id="checkbox"
						@change="$emit('handleUpdateDraggable', ($event.target as HTMLInputElement).checked)"
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
</template>

<script setup lang="ts">
	import { computed, onMounted, watch } from 'vue';
	import Select from '@/components/general/Select.vue';
	import TextField from '@/components/general/TextField.vue';
	import Dropdown from '@/components/general/Dropdown.vue';
	import { MenuItem } from '@headlessui/vue';
	import { useDebounceFn } from '@vueuse/core';
	import {
		MagnifyingGlassIcon,
		ChevronDownIcon,
		ArrowPathIcon,
		XMarkIcon,
	} from '@heroicons/vue/24/outline';

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
		chosenUser?: object | null;
		activeDraggable?: boolean;
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

	const debouncedRouterPush = useDebounceFn(() => {
		router.push({
			...currentRoute,
			query: buildQuery(),
		});
	}, 300);

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
		debouncedRouterPush();
	});

	watch(selectedCategory, () => {
		emit(
			'handleChosenCategory',
			props.categories.find((option) => option.id === selectedCategory.value),
		);
		debouncedRouterPush();
	});

	watch(searchText, (newValue) => {
		emit('handleSearchTextChanged', newValue);
		debouncedRouterPush();
	});
	const loadTasks = () => {
		emit('loadColumns');
		emit('loadTasks');
	};
</script>
