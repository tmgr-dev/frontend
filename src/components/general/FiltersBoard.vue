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
				v-if="
					isFeatureEnabled('categories') &&
					isUserFeatureEnabled('board.category_filter') &&
					categories.length >= 2
				"
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

			<div v-if="showUserFilter" class="w-full">
				<UiSelect v-model="selectedUserValue">
					<UiSelectTrigger class="w-full">
						<UiSelectValue placeholder="Select user" />
					</UiSelectTrigger>
					<UiSelectContent>
						<UiSelectItem
							v-for="u in workspaceUsers"
							:key="u.id"
							:value="String(u.id)"
						>
							<span class="flex min-w-0 items-center gap-2">
								<UserAvatar
									v-if="u.id !== 0"
									:user-id="u.id"
									:name="u.name"
									:has-avatar="u.has_avatar ?? false"
									:size="20"
								/>
								<span class="truncate">{{ u.name }}</span>
							</span>
						</UiSelectItem>
					</UiSelectContent>
				</UiSelect>
			</div>

			<div v-if="showPersonaFilter" class="w-full">
				<Select
					placeholder="Select persona"
					:options="mobilePersonaOptions"
					v-model="selectedPersona"
					label-key="name"
					value-key="id"
				/>
			</div>

			<button
				v-if="hasActiveFilters"
				type="button"
				@click="clearFilters"
				class="flex w-full items-center justify-center gap-2 rounded-md border border-line bg-surface px-4 py-2 text-sm text-ink transition-colors hover:bg-surface-hover"
			>
				<span class="material-icons text-base">clear</span>
				<span>Clear filters</span>
			</button>

			<div class="mt-3 flex flex-col gap-3 border-t border-line pt-3">
				<button
					type="button"
					@click="handleMobileReorderClick"
					class="w-full rounded-md border border-line bg-surface px-4 py-2 text-left text-sm text-ink transition-colors hover:bg-surface-hover"
				>
					Reorder statuses
				</button>
				<slot name="actions-start"></slot>
			</div>
		</div>
	</div>

	<!-- Desktop pill-style filters (header layout) -->
	<div v-else data-tauri-drag-region class="flex min-w-0 items-center gap-2">
		<!-- Search -->
		<div
			v-if="isUserFeatureEnabled('board.search_input')"
			class="relative min-w-0"
		>
			<input
				v-if="searchOpen || searchText"
				ref="searchInput"
				type="text"
				v-model="searchText"
				placeholder="Search tasks…"
				class="h-9 w-44 rounded-pill border border-line bg-surface pl-9 pr-3 text-sm text-ink outline-none placeholder:text-ink-subtle focus:border-line-strong lg:w-56"
				@blur="searchOpen = false"
				@keydown.esc="closeSearch"
			/>
			<button
				type="button"
				title="Search"
				:class="[
					'flex h-9 w-9 items-center justify-center rounded-pill text-ink-subtle hover:bg-surface-hover hover:text-ink',
					(searchOpen || searchText) &&
						'pointer-events-none absolute left-0 top-0',
				]"
				@click="openSearch"
			>
				<MagnifyingGlassIcon class="h-4 w-4" />
			</button>
		</div>

		<!-- Category / user filters -->
		<Popover v-if="hasSelectFilters" class="relative shrink-0">
			<PopoverButton
				class="flex h-9 items-center gap-2 rounded-pill border border-line bg-surface px-3 text-sm text-ink outline-none hover:bg-surface-hover"
			>
				<FunnelIcon class="h-4 w-4 text-ink-subtle" />
				Filters
				<span
					v-if="activeSelectCount"
					class="rounded-full bg-brand px-1.5 text-xs font-semibold leading-5 text-white"
				>
					{{ activeSelectCount }}
				</span>
			</PopoverButton>
			<PopoverPanel
				class="absolute right-0 z-50 mt-2 flex w-64 flex-col gap-2 rounded-xl border border-line bg-surface p-3 shadow-lg"
			>
				<!-- Category select -->
				<div v-if="showCategoryFilter" class="relative">
					<select
						v-model.number="selectedCategory"
						class="h-9 w-full appearance-none rounded-pill border border-line bg-surface pl-3 pr-9 text-sm text-ink outline-none"
					>
						<option :value="0">All categories</option>
						<option
							v-for="cat in categoryOptions"
							:key="cat.id"
							:value="cat.id"
						>
							{{ cat.title }}
						</option>
					</select>
					<ChevronDownIcon
						class="pointer-events-none absolute right-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink-subtle"
					/>
				</div>

				<!-- User select -->
				<div v-if="showUserFilter" class="relative">
					<UiSelect v-model="selectedUserValue">
						<UiSelectTrigger
							class="h-9 rounded-pill border-line bg-surface px-3 text-ink"
						>
							<UiSelectValue placeholder="All users" />
						</UiSelectTrigger>
						<UiSelectContent>
							<UiSelectItem value="0">All users</UiSelectItem>
							<UiSelectItem
								v-for="u in userOptions"
								:key="u.id"
								:value="String(u.id)"
							>
								<span class="flex min-w-0 items-center gap-2">
									<UserAvatar
										:user-id="u.id"
										:name="u.name"
										:has-avatar="u.has_avatar ?? false"
										:size="20"
									/>
									<span class="truncate">{{ u.name }}</span>
								</span>
							</UiSelectItem>
						</UiSelectContent>
					</UiSelect>
				</div>

				<!-- Persona select -->
				<div v-if="showPersonaFilter" class="relative">
					<select
						v-model="selectedPersona"
						data-testid="persona-filter"
						class="h-9 w-full appearance-none rounded-pill border border-line bg-surface pl-3 pr-9 text-sm text-ink outline-none"
					>
						<option value="">All personas</option>
						<option
							v-for="option in personaFilterOptions"
							:key="option.id"
							:value="option.id"
						>
							{{ option.name }}
						</option>
					</select>
					<ChevronDownIcon
						class="pointer-events-none absolute right-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink-subtle"
					/>
				</div>
			</PopoverPanel>
		</Popover>

		<!-- Separator -->
		<div class="h-5 w-px shrink-0 bg-line"></div>

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
		<Dropdown data-tour="statuses.editor">
			<MenuItem v-slot="{ active }">
				<div
					class="flex items-center px-3 py-2 text-sm text-ink"
					:class="active ? 'bg-surface-hover' : ''"
				>
					<input
						class="h-4 w-4 cursor-pointer rounded"
						type="checkbox"
						id="checkbox"
						@change="
							$emit(
								'handleUpdateDraggable',
								($event.target as HTMLInputElement).checked,
							)
						"
						:checked="activeDraggable"
					/>
					<label class="ml-2 cursor-pointer text-sm" for="checkbox"
						>Reorder statuses</label
					>
				</div>
			</MenuItem>
			<MenuItem v-slot="{ active }">
				<slot name="actions-start" :active="active"></slot>
			</MenuItem>
		</Dropdown>
	</div>
</template>

<script setup lang="ts">
	import Dropdown from '@/components/general/Dropdown.vue';
	import Select from '@/components/general/Select.vue';
	import TextField from '@/components/general/TextField.vue';
	import UserAvatar from '@/components/general/UserAvatar.vue';
	import {
		Select as UiSelect,
		SelectContent as UiSelectContent,
		SelectItem as UiSelectItem,
		SelectTrigger as UiSelectTrigger,
		SelectValue as UiSelectValue,
	} from '@/components/ui/select';
	import { MY_PERSONAS_FILTER } from '@/utils/boardLoading';
	import {
		MenuItem,
		Popover,
		PopoverButton,
		PopoverPanel,
	} from '@headlessui/vue';
	import {
		ArrowPathIcon,
		ChevronDownIcon,
		FunnelIcon,
		MagnifyingGlassIcon,
		XMarkIcon,
	} from '@heroicons/vue/24/outline';
	import { computed, nextTick, ref } from 'vue';

	export interface UserOption {
		id: number;
		name: string;
		value: number;
		label: string;
		has_avatar?: boolean;
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
		personaOptions?: Array<{ id: string; name: string }>;
		activeDraggable?: boolean;
		categories: CategoryOption[];
		isMobileModal?: boolean;
	}

	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import { useStore } from 'vuex';

	interface State {
		selectedCategory: number;
		searchText: string | null;
		selectedUser: number;
		selectedPersona: string;
	}
	const props = withDefaults(defineProps<Props>(), {
		personaOptions: () => [],
	});

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
	const { isFeatureEnabled, isUserFeatureEnabled, showSurface } =
		useFeatureToggles();

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

	const selectedUserValue = computed({
		get: () => String(selectedUser.value || 0),
		set: (value) => {
			selectedUser.value = Number(value) || 0;
		},
	});

	const selectedPersona = computed({
		get: () =>
			(store.state as unknown as { filter: State }).filter.selectedPersona,
		set: (value) => {
			store.commit('updateSelectedPersona', value ?? '');
		},
	});

	const hasActiveFilters = computed(() => {
		return !!(
			searchText.value ||
			selectedCategory.value ||
			selectedUser.value ||
			selectedPersona.value
		);
	});

	const categoryOptions = computed(() =>
		props.categories.filter((c) => c.id !== 0),
	);
	const userOptions = computed(() =>
		props.workspaceUsers.filter((u) => u.id !== 0),
	);

	const clearFilters = () => {
		searchText.value = '';
		selectedCategory.value = 0;
		selectedUser.value = 0;
		selectedPersona.value = '';
	};
	const loadTasks = () => emit('loadTasks');

	const searchOpen = ref(false);
	const searchInput = ref<HTMLInputElement | null>(null);
	const openSearch = async () => {
		searchOpen.value = true;
		await nextTick();
		searchInput.value?.focus();
	};
	const closeSearch = () => {
		searchText.value = '';
		searchOpen.value = false;
	};

	const showCategoryFilter = computed(
		() =>
			isFeatureEnabled('categories') &&
			isUserFeatureEnabled('board.category_filter') &&
			props.categories.length >= 2,
	);
	const showUserFilter = computed(
		() =>
			showSurface('assignees.board-filter') &&
			isUserFeatureEnabled('board.user_filter') &&
			props.workspaceUsers.length >= 2,
	);
	const showPersonaFilter = computed(
		() =>
			isFeatureEnabled('personas') &&
			isUserFeatureEnabled('board.user_filter') &&
			props.personaOptions.length > 0,
	);
	const personaFilterOptions = computed(() => [
		{ id: MY_PERSONAS_FILTER, name: "My personas' queue" },
		...props.personaOptions,
	]);
	const mobilePersonaOptions = computed(() => [
		{ id: '', name: 'All personas' },
		...personaFilterOptions.value,
	]);
	const hasSelectFilters = computed(
		() =>
			showCategoryFilter.value ||
			showUserFilter.value ||
			showPersonaFilter.value,
	);
	const activeSelectCount = computed(
		() =>
			(selectedCategory.value ? 1 : 0) +
			(selectedUser.value ? 1 : 0) +
			(selectedPersona.value ? 1 : 0),
	);
</script>
