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
	import Button from '@/components/general/Button.vue';
	import {
		Dialog,
		DialogContent,
		DialogHeader,
		DialogTitle,
		DialogTrigger,
	} from '@/components/ui/dialog/index';
	import CategoriesCombobox from '@/components/CategoriesCombobox.vue';

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
				{ id: 0, title: 'All categories' },
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

	watch(selectedCategory, async (newCategoryId) => {
		await loadTasks();
		tasks.value = tasks.value.filter(
			(task) => task.category && task.category.id === newCategoryId,
		);
	});

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
					},
				});
			} else {
				fetchedTasks = await getTasks({
					params: {
						search: searchText.value,
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
			<div class="items-center justify-between px-2 md:flex md:flex-nowrap">
				<transition name="fade">
					<div
						v-if="summaryTimeString"
						class="text-bold my-5 mr-6 shrink-0 text-center text-lg text-opacity-25 sm:w-auto sm:text-xl lg:text-2xl"
					>
						{{ summaryTimeString }}
					</div>
				</transition>

				<div
					class="absolute top-14 flex w-[90%] items-center justify-center p-4 md:static"
				>
					<div
						class="mr-3 w-1/2 overflow-hidden md:ml-auto md:w-full lg:w-1/3 xl:w-1/5"
					>
						<TextField
							placeholder="search by task name"
							v-model="searchText"
							class="p-2 md:p-0"
						/>
					</div>
				</div>

				<div
					class="ml-0 mt-14 w-full items-center gap-2 text-center sm:w-auto md:mt-0 md:flex"
				>
					<Dialog>
						<DialogTrigger as-child>
							<button
								@click="showCategorySelect = !showCategorySelect"
								type="button"
								title="filters"
							>
								<SlidersHorizontalIcon class="size-6 stroke-gray-400" />
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

							<!--							<DialogFooter>
								<button
									type="submit"
									class="w-full rounded bg-tmgr-light-blue p-2 text-white hover:opacity-90"
								>
									Save
								</button>
							</DialogFooter>-->
						</DialogContent>
					</Dialog>

					<button
						type="button"
						title="Tasks selection mode"
						@click="selectableTasks = !selectableTasks"
					>
						<SquareDashedMousePointerIcon
							class="size-7"
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
		</template>
	</BaseLayout>
</template>
