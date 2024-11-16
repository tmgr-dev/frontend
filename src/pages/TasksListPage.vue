<script setup>
	import LoadingTasksList from '@/components/loaders/LoadingTasksList.vue';
	import TasksListComponent from '@/components/tasks/TasksListComponent.vue';
	import Confetti from '@/components/Confetti.vue';
	import { getTasks, getTasksByStatus } from '@/actions/tmgr/tasks';
	import TextField from '@/components/general/TextField.vue';
	import { getCategories } from '@/actions/tmgr/categories';
	import { computed, onMounted, ref, watch } from 'vue';
	import { useRoute } from 'vue-router';
	import {
		SquareDashedMousePointerIcon,
		SlidersHorizontalIcon,
	} from 'lucide-vue-next';
	import {
		Dialog,
		DialogContent,
		DialogHeader,
		DialogTitle,
		DialogTrigger,
		DialogFooter,
	} from '@/components/ui/dialog';
	import CategoriesCombobox from '@/components/CategoriesCombobox.vue';
	import { Button } from '@/components/ui/button';
	import store from '@/store';

	const route = useRoute();
	const selectableTasks = ref(false);
	const errorLoading = ref(false);
	const searchText = ref(null);
	const searchTimeout = ref(null);
	const summaryTimeString = ref(null);
	const isLoading = ref(true);
	const h1 = {
		CurrentTasksList: 'Current tasks',
		HiddenTasksList: 'Hidden tasks',
		ArchiveTasksList: 'Archive tasks',
	};
	const tasks = ref([]);
	const isLoadingActions = ref({});
	const hasAbilityToShowConfetti = ref(false);
	const categories = ref([]);
	const selectedCategory = ref(null);
	const showCategorySelect = ref(false);

	const status = computed(() => route.meta.status);

	onMounted(async () => {
		try {
			const categoriesData = await getCategories();

			categories.value = [
				{ id: -1, title: 'All categories' },
				...categoriesData.map((category) => ({
					id: category.id,
					title: category.title,
				})),
			];
			const data = await loadTasks();
			setLoadingActions(data);
		} catch (e) {
			console.error(e);
		}
	});

	watch(searchText, () => {
		clearTimeout(searchTimeout.value);
		searchTimeout.value = setTimeout(loadTasks, 500);
	});

	watch(selectedCategory, loadTasks);
	watch(() => store.state.reloadTasksKey, loadTasks);

	function setLoadingActions(tasks) {
		tasks.forEach((task) => {
			isLoadingActions.value[`hide-${task.id}`] = false;
			isLoadingActions.value[`done-${task.id}`] = false;
			isLoadingActions.value[`start-${task.id}`] = false;
			isLoadingActions.value[`stop-${task.id}`] = false;
			isLoadingActions.value[`activate-${task.id}`] = false;
			isLoadingActions.value[`delete-${task.id}`] = false;
		});
	}

	async function reloadTasks() {
		hasAbilityToShowConfetti.value = true;
		await loadTasks();
	}

	async function loadTasks() {
		try {
			isLoading.value = true;
			clearTimeout(searchTimeout.value);

			let fetchedTasks = [];

			if (status.value) {
				fetchedTasks = await getTasksByStatus(status.value, {
					params: {
						search: searchText.value,
						project_category_id:
							selectedCategory.value === -1 ? null : selectedCategory.value,
					},
				});
			} else {
				fetchedTasks = await getTasks({
					params: {
						search: searchText.value,
						project_category_id:
							selectedCategory.value === -1 ? null : selectedCategory.value,
					},
				});
			}

			summaryTimeString.value = formatTime(
				fetchedTasks.reduce((summary, task) => task.common_time + summary, 0),
			);
			tasks.value = fetchedTasks;

			return fetchedTasks;
		} catch (e) {
			console.error(e);
			errorLoading.value = true;
		} finally {
			isLoading.value = false;
		}
	}

	function formatTime(taskTime) {
		let hours = Math.floor(taskTime / 3600);
		let minutes = Math.ceil((taskTime % 3600) / 60);

		return `${
			hours > 0 ? hours + ' hour' + (hours > 1 ? 's' : '') : ''
		} ${minutes} minute${minutes > 1 ? 's' : ''}`;
	}
</script>

<template>
	<teleport to="title">
		{{ h1[route.name] }}
	</teleport>

	<BaseLayout>
		<template #action>
			<div class="flex flex-wrap items-center justify-between gap-2 px-2">
				<transition name="fade">
					<div
						v-if="summaryTimeString"
						class="text-bold mr-6 shrink-0 text-center text-lg text-opacity-25 sm:text-xl lg:text-2xl"
					>
						{{ summaryTimeString }}
					</div>
				</transition>

				<div class="ml-auto flex items-center gap-2 text-center">
					<TextField
						placeholder="search by task name"
						v-model="searchText"
						class="md:p-0"
					/>

					<Dialog>
						<DialogTrigger as-child>
							<button
								@click="showCategorySelect = !showCategorySelect"
								type="button"
								title="filters"
								class="flex size-8 rounded border p-1.5"
								:class="[
									selectedCategory &&
										selectedCategory !== -1 &&
										'border-gray-900 dark:border-white',
								]"
							>
								<SlidersHorizontalIcon
									class="m-auto size-full stroke-gray-400"
									:class="[
										selectedCategory &&
											selectedCategory !== -1 &&
											'stroke-gray-900 dark:stroke-white',
									]"
								/>
							</button>
						</DialogTrigger>

						<DialogContent
							class="!rounded-[8px] bg-white dark:border-transparent dark:bg-gray-900 dark:text-white sm:max-w-[425px]"
						>
							<DialogHeader>
								<DialogTitle>Filters</DialogTitle>
							</DialogHeader>

							<CategoriesCombobox
								:categories="categories"
								v-model="selectedCategory"
								class="!w-full"
							/>
						</DialogContent>

						<DialogFooter> </DialogFooter>
					</Dialog>

					<button
						@click="selectableTasks = !selectableTasks"
						type="button"
						title="Tasks selection mode"
						class="flex size-8 rounded border p-1.5"
						:class="[selectableTasks && 'border-gray-900 dark:border-white']"
					>
						<SquareDashedMousePointerIcon
							class="m-auto size-full"
							:class="[
								selectableTasks
									? 'stroke-tmgr-blue dark:stroke-white'
									: 'stroke-gray-400',
							]"
						/>
					</button>
				</div>
			</div>
		</template>

		<template #body>
			<div class="mt-4">
				<tasks-list-component
					v-if="tasks && tasks.length > 0 && !isLoading"
					:tasks="tasks"
					:status="status"
					:is-loading-actions="isLoadingActions"
					:has-selectable="selectableTasks"
					@reload-tasks="reloadTasks"
					ref="tasksListComponent"
				/>

				<div v-else-if="errorLoading" class="text-center text-xl italic">
					Something went wrong...
				</div>

				<div v-else-if="!isLoading" class="text-center text-xl italic">
					You don't have tasks here

					<confetti v-if="hasAbilityToShowConfetti" />
				</div>

				<loading-tasks-list v-if="isLoading" class="mx-2" />
			</div>
		</template>
	</BaseLayout>
</template>
