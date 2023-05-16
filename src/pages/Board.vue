<template>
	<teleport to="title">{{ title }}</teleport>

	<BaseLayout no-copyright>
		<template #body>
			<div class="block justify-center">
				<div class="w-full overflow-x-auto">
					<div class="min-h-15">
						<div class="md:hidden relative">
							<filters-svg
								@handleFilter="handleFilter"
								:isFiltersModal="isFiltersModal"
							/>

							<Transition name="bounce-right-fade">
								<Modal
									v-if="isFiltersModal"
									modal-class="p-6 w-96"
									close-on-bg-click
									@close="closeFilterModal"
								>
									<template #modal-body>
										<FiltersBoard
											v-if="workspaceUsers.length"
											:workspaceUsers="workspaceUsers"
											:categories="categories"
											:chosen-user.sync="chosenUser"
											@update:chosenUser="handleChosenUserUpdate"
											@handleChosenCategory="handleChosenCategory"
											:activeDraggable="activeDraggable"
											@handleUpdateDraggable="handleUpdateDraggable"
											@handleSearchTextChanged="handleSearchTextChanged"
										/>
									</template> </Modal
							></Transition>
						</div>
						<div class="hidden md:block">
							<FiltersBoard
								v-if="workspaceUsers.length"
								:workspaceUsers="workspaceUsers"
								:categories="categories"
								:chosen-user.sync="chosenUser"
								@update:chosenUser="handleChosenUserUpdate"
								@handleChosenCategory="handleChosenCategory"
								:activeDraggable="activeDraggable"
								@handleUpdateDraggable="handleUpdateDraggable"
								@handleSearchTextChanged="handleSearchTextChanged"
							/>
						</div>
					</div>
					<div class="board-container">
						<Draggable
							:disabled="!activeDraggable"
							v-model="columns"
							group="columns"
							item-key="id"
							class="board-container"
							@end="onMove"
							data-id="column"
						>
							<template #item="{ element: column }">
								<div class="board-container__item pr-2">
									<div class="column-width h-full rounded-lg px-3 pb-3">
										<div>
											<div
												class="relative flex items-center rounded pt-2 pl-2 pb-2 font-sans text-sm font-semibold tracking-wide text-tmgr-blue dark:text-tmgr-gray"
												:style="{
													'border-top': `solid 5px ${column.status.color}`,
												}"
											>
												<div
													v-if="activeDraggable"
													class="mr-2 flex items-center hover:cursor-move"
												>
													<EllipsisVerticalIcon
														class="h-3"
														aria-hidden="true"
													/>
													<EllipsisVerticalIcon
														class="-ml-2 h-3"
														aria-hidden="true"
													/>
												</div>

												<div
													class="group w-full relative flex items-center h-6"
												>
													<span class="text-sm">
														{{ column.title }}
													</span>

													<div
														v-tooltip.left="
															userSettings.showTooltips
																? `Create task `
																: { visible: false }
														"
														class="opacity-0 group-hover:opacity-100 transition duration-700 absolute top-0 right-3"
													>
														<div
															@click="openTaskModal(column)"
															class="material-icons text-gray-500 cursor-pointer hover:text-black dark:text-gray-700 dark:hover:text-white"
														>
															add
														</div>
													</div>
												</div>

												<Dropdown class="ml-auto pr-2">
													<MenuItem>
														<a
															href="#"
															class="block px-4 py-2 text-sm text-neutral-600"
															@click.prevent="openTaskModal(column)"
														>
															Create a task
														</a>
													</MenuItem>
													<MenuItem>
														<a
															href="#"
															class="block px-4 py-2 text-sm text-neutral-600"
															@click.prevent="openStatusModal(column)"
														>
															Edit status
														</a>
													</MenuItem>
												</Dropdown>
											</div>
										</div>

										<Draggable
											v-model="column.tasks"
											:animation="200"
											ghost-class="ghost-card"
											group="tasks"
											item-key="id"
											@end="onEnd"
											:data-status="column.status.id"
											class="board-card"
										>
											<template #item="{ element: task }">
												<TaskBoardCard
													:task="task"
													class="my-5 cursor-move"
													:data-task="jsonEncode(task)"
												/>
											</template>
										</Draggable>
									</div>
								</div>
							</template>
						</Draggable>
						<div class="w-12 h-12 flex justify-center items-center">
							<span
								@click="
									() => {
										isShowStatusModal = true;
										isCreatingStatus = true;
									}
								"
								class="material-icons text-2xl text-gray-500 cursor-pointer hover:text-black dark:text-gray-700 dark:hover:text-white"
							>
								add
							</span>
						</div>
					</div>
				</div>

				<Transition name="bounce-right-fade">
					<Modal
						v-if="isShowStatusModal"
						modal-class="p-6 w-96"
						close-on-bg-click
						@close="closeModal"
					>
						<template #modal-body>
							<div>
								<div v-if="!isShowColorPicker">
									<h1 v-if="isCreatingStatus" class="text-xl text-center mb-3">
										Create status
									</h1>
									<h1 v-else class="text-xl text-center mb-3">Edit status</h1>
									<label class="flex flex-col gap-2 font-medium mb-3">
										Status name :
										<TextField
											placeholder="Name"
											v-model="statusName"
											:errors="errors.name"
										/>
									</label>
									<label class="flex flex-col gap-2 font-medium mb-4">
										Status type :
										<Select
											placeholder="Select Type"
											:options="statusTypes"
											v-model="statusType"
											:errors="errors.type"
											label-key="name"
											value-key="name"
										/>
									</label>
									<div
										class="flex mb-3 items-center justify-between font-medium mb-3"
									>
										<span>Status color :</span>
										<button
											type="button"
											:style="{ backgroundColor: statusColor }"
											class="w-2/4 rounded py-2 px-4 font-bold text-white outline-none transition sm:mb-0"
											:class="'bg-' + '[' + statusColor + ']'"
											@click="isShowColorPicker = true"
										>
											{{ statusColor }}
										</button>
									</div>

									<button
										@click="saveNewStatus()"
										class="mt-3 w-full rounded bg-orange-500 py-2 px-4 font-bold text-white outline-none transition hover:bg-orange-600 sm:mb-0"
										type="button"
									>
										{{ isCreatingStatus ? 'Create' : 'Save' }}
									</button>

									<button
										v-if="!isCreatingStatus"
										@click="deleteStatus"
										class="mt-3 w-full rounded bg-red-500 py-2 px-4 font-bold text-white outline-none transition hover:bg-red-700 sm:mb-0"
										type="button"
									>
										Delete
									</button>
								</div>
								<div class="p-8" v-if="isShowColorPicker">
									<color-picker
										:hue="color.hue"
										@input="onInput"
										@select="onColorSelect"
									></color-picker>
								</div>
							</div>
						</template>
					</Modal>
				</Transition>
			</div>
			<Transition name="fade">
				<confirm
					v-if="confirm"
					:title="confirm.title"
					:body="confirm.body"
					@onOk="confirm.action()"
					@onCancel="confirm = undefined"
				/>
			</Transition>
		</template>
	</BaseLayout>
</template>

<script>
	import Modal from 'src/components/Modal.vue';
	import Button from 'src/components/general/Button.vue';
	import Draggable from 'vuedraggable';
	import DropdownMenu from 'src/components/general/DropdownMenu.vue';

	import {
		getWorkspaceMembers,
		getWorkspaces,
		getWorkspaceStatuses,
		updateWorkspaceOrder,
	} from 'src/actions/tmgr/workspaces';
	import {
		getSortedTasksByStatus,
		updateTaskOrders,
		updateTaskStatus,
	} from 'src/actions/tmgr/tasks';
	import { getUser, updateUser } from 'src/actions/tmgr/user';
	import FiltersBoard from 'src/components/general/FiltersBoard.vue';
	import Dropdown from 'src/components/general/Dropdown.vue';
	import { MenuItem } from '@headlessui/vue';
	import { EllipsisVerticalIcon } from '@heroicons/vue/20/solid';
	import TaskBoardCard from 'src/components/tasks/TaskBoardCard.vue';
	import TextField from 'src/components/general/TextField.vue';
	import {
		createStatus,
		deleteStatus,
		updateStatus,
	} from 'src/actions/tmgr/statuses';
	import ColorPicker from '@radial-color-picker/vue-color-picker';
	import Select from 'src/components/general/Select.vue';
	import Confirm from 'src/components/general/Confirm.vue';
	import { hslToHex, hueFromHex } from 'src/utils/convertColors';
	import { getCategories } from 'src/actions/tmgr/categories';
	import FiltersSvg from 'src/components/UI/FiltersSvg.vue';

	export default {
		name: 'Board',
		components: {
			FiltersSvg,
			TaskBoardCard,
			EllipsisVerticalIcon,
			MenuItem,
			Dropdown,
			TextField,
			Modal,
			FiltersBoard,
			DropdownMenu,
			Button,
			Draggable,
			ColorPicker,
			Select,
			Confirm,
		},

		data: () => ({
			title: 'Board',
			user: {
				name: null,
				password: null,
				password_confirmation: null,
			},
			userData: {},
			workspacesData: [],
			workspaceId: 0,
			workspaceUsers: [],
			categories: [],
			chosenUser: null,
			chosenCategory: null,
			errors: {},
			archivedStatus: null,
			searchText: '',
			filteredTasksArray: [],
			isShowStatusModal: false,
			isCreatingStatus: false,
			statusName: '',
			statusType: '',
			statusColor: '#077fe8',
			statusId: '',
			oldColumns: [
				{
					title: 'Backlog',
					status: 'created',
					tasks: [],
				},
				{
					title: 'In Progress',
					status: 'active',
					tasks: [],
				},
				{
					title: 'Hidden',
					status: 'hidden',
					tasks: [],
				},
			],
			columns: [],
			activeDraggable: false,
			color: {
				hue: 235,
				saturation: 100,
				luminosity: 50,
				alpha: 1,
			},
			isShowColorPicker: false,
			statusTypes: [
				{ id: 1, name: 'active' },
				{ id: 2, name: 'hidden' },
				{ id: 3, name: 'archived' },
				{ id: 4, name: 'completed' },
			],
			confirm: null,
			tasks: [],
			isFiltersModal: false,
		}),
		watch: {
			'$store.state.reloadTasksKey'() {
				this.loadTasks();
			},
			chosenUser: function () {
				this.loadTasks();
			},
			searchText: function () {
				this.loadTasks();
			},
			chosenCategory: function () {
				this.loadTasks();
			},
		},
		computed: {
			userSettings() {
				return this.$store.state.userSettings ?? {};
			},
		},
		methods: {
			onColorSelect() {
				const hexColor = hslToHex(
					this.color.hue,
					this.color.saturation,
					this.color.luminosity,
				);
				this.statusColor = hexColor;
				setTimeout(() => {
					this.isShowColorPicker = false;
				}, 1000);
			},
			handleFilter() {
				this.isFiltersModal = !this.isFiltersModal;
			},
			closeFilterModal() {
				this.isFiltersModal = false;
			},

			onInput(hue) {
				this.color.hue = hue;
			},
			handleChosenUserUpdate(newChosenUser) {
				this.chosenUser = newChosenUser;
			},
			handleChosenCategory(newChosenCategory) {
				this.chosenCategory = newChosenCategory;
			},
			closeModal() {
				this.clearStatus();
				this.isShowStatusModal = false;
				this.isShowColorPicker = false;
				this.color.hue = hueFromHex(this.statusColor);
			},
			clearStatus() {
				this.statusName = '';
				this.statusType = '';
				this.statusColor = '#077fe8';
				this.errors = {};
			},
			async saveNewStatus() {
				const newStatus = {
					name: this.statusName,
					type: this.statusType,
					color: this.statusColor,
				};

				if (this.isCreatingStatus) {
					try {
						await createStatus(this.workspaceId, newStatus);
						this.showAlert('Save', 'The status was created');
						this.closeModal();
						this.$store.commit('rerenderApp');
					} catch (error) {
						if (error) {
							this.errors = error.response?.data?.errors;
						}
					}
				}
				if (!this.isCreatingStatus) {
					try {
						await updateStatus(this.statusId, newStatus);
						this.showAlert('Saved', 'The status was edited');
						this.closeModal();
						this.$store.commit('rerenderApp');
					} catch (error) {
						if (error) {
							this.errors = error.response?.data?.errors;
						}
					}
				}
			},
			showConfirm(title, body, action) {
				this.confirm = { title, body, action };
			},
			async deleteStatus() {
				const deleteStatusConfirmation = async () => {
					await deleteStatus(this.statusId);
					this.closeModal();
					this.showAlert('Saved', 'The status was deleted');
					this.$store.commit('rerenderApp');
				};
				this.showConfirm(
					'Delete status',
					'Are you sure?',
					deleteStatusConfirmation,
				);
			},
			openStatusModal(column) {
				this.isShowStatusModal = true;
				this.statusType = column.status.type;
				this.statusName = column.status.name;
				this.statusColor = column.status.color;
				this.statusId = column.status.id;
				this.color.hue = hueFromHex(this.statusColor);
			},

			openTaskModal(column) {
				this.$store.commit('setShowCreatingTaskModal', column.status.id);
			},
			jsonEncode(data) {
				return JSON.stringify(data);
			},
			jsonDecode(stringData) {
				return JSON.parse(stringData);
			},
			async onEnd({
				to: {
					dataset: { status },
				},
				item: {
					dataset: { task },
				},
			}) {
				task = this.jsonDecode(task);
				status = parseInt(status);

				if (task.status_id !== status) {
					await updateTaskStatus(task.id, status);
					const foundTask = this.findTask(status, task.id);
					foundTask.status_id = status;
				}
				setTimeout(() => this.saveOrders(status), 500);
			},
			async onMove() {
				const sortedStats = this.columns.map((col, index) => {
					col.status.pivot.order = index + 1;
					const { status_id, order } = col.status.pivot;

					return { status_id, order };
				});

				await updateWorkspaceOrder(this.workspaceId, {
					statuses_with_order: sortedStats,
				});
			},
			async saveOrders(status) {
				const column = this.findColumn(status);
				if (!column) {
					return;
				}
				const orders = [];
				const { tasks } = column;
				tasks.forEach(({ id }, index) => {
					orders.push({
						id: id,
						order: index + 1,
					});
				});

				await updateTaskOrders({
					tasks: orders,
				});
			},
			findTask(status, taskId) {
				const column = this.findColumn(status);
				if (!column) {
					return null;
				}
				const { tasks } = column;
				for (let i = 0; i < tasks.length; ++i) {
					const task = tasks[i];
					if (task.id === taskId) {
						return task;
					}
				}
				return null;
			},
			findColumn(status) {
				const { columns } = this;
				for (let i = 0; i < columns.length; ++i) {
					const column = columns[i];
					if (column.status.id === status) {
						return column;
					}
				}
				return null;
			},
			async loadColumns() {
				const columns = [];
				const statuses = await getWorkspaceStatuses();
				statuses.sort((a, b) => a.pivot.order - b.pivot.order);

				for (let i = 0; i < statuses.length; ++i) {
					let status = statuses[i];
					if (status.type === 'archived') {
						this.archivedStatus = status;
						continue;
					}

					columns.push({
						title: status.name,
						status: status,
						tasks: [],
					});
				}

				await this.$nextTick(() => {
					this.columns = columns;
				});
			},
			handleSearchTextChanged(newValue) {
				this.searchText = newValue;
			},
			async loadTasks() {
				const tasksPromises = this.columns.map((column) =>
					this.loadTasksByStatus(column.status),
				);

				const tasksArray = await Promise.all(tasksPromises);
				if (this.searchText === '') {
					this.filteredTasksArray = tasksArray;
				} else {
					this.filteredTasksArray = tasksArray.map((el) =>
						el.filter(
							(item) =>
								item.title
									.toLowerCase()
									.includes(this.searchText.toLowerCase()) ||
								item.description
									.toLowerCase()
									.includes(this.searchText.toLowerCase()),
						),
					);
				}

				this.columns.forEach((column, i) => {
					column.tasks = this.filteredTasksArray[i];
				});
			},
			async loadTasksByStatus(status) {
				this.tasks = await getSortedTasksByStatus(status?.id || status, {
					params: {
						'order[column]': 'order',
						'order[direction]': 'asc',
					},
				});
				if (this.chosenUser?.id) {
					this.tasks = this.tasks.filter((item) =>
						item.assignees.find(
							(assignee) =>
								assignee.id === this.chosenUser.id &&
								assignee.name === this.chosenUser.name,
						),
					);
				}
				if (this.chosenCategory?.id) {
					this.tasks = this.tasks.filter(
						(task) => task.project_category_id === this.chosenCategory.id,
					);
				}
				return this.tasks;
			},
			async saveUser() {
				try {
					this.user = await updateUser(this.user);
					this.showAlert('Saved', 'User data saved');
				} catch (e) {
					this.errors = e.response?.data?.errors || {};
				}
			},
			handleUpdateDraggable(value) {
				this.activeDraggable = value;
				this.$store.commit('updateDraggable', value);
			},
		},
		async beforeMount() {
			const user = await getUser();
			this.workspacesData = await getWorkspaces();
			this.userData = user;

			const workspaceSetting = this.userData.settings.find(
				(setting) => setting.key === 'current_workspace',
			);
			if (workspaceSetting) {
				this.workspaceId = +workspaceSetting.value;
				const users = await getWorkspaceMembers(this.workspaceId);
				this.workspaceUsers = [{ id: 0, name: 'All users' }, ...users];
			}
		},
		async mounted() {
			// this.activeDraggable = this.$store.state.isDraggable;
			const categoriesData = await getCategories();
			this.categories = [
				{ id: 0, title: 'All categories' },
				...categoriesData.map((cat) => ({
					id: cat.id,
					title: cat.title,
				})),
			];
			document.body.classList.add('overflow-hidden');
			await this.loadColumns();
			await this.loadTasks();
			this.color.hue = hueFromHex(this.statusColor);
		},
		unmounted() {
			document.body.classList.remove('overflow-hidden');
		},
	};
</script>

<style lang="scss" scoped>
	@import '@radial-color-picker/vue-color-picker/dist/vue-color-picker.min.css';
	.settings-container {
		max-width: 700px;
		margin: 50px auto;
		padding: 20px;
		box-shadow: rgb(233 233 233) 1px 4px 20px;
	}

	.reset-scroll {
		&::-webkit-scrollbar {
			all: initial !important;
		}
		&::-webkit-scrollbar-button {
			all: initial !important;
		}
		&::-webkit-scrollbar-track {
			all: initial !important;
		}
		&::-webkit-scrollbar-track-piece {
			all: initial !important;
		}
		&::-webkit-scrollbar-thumb {
			all: initial !important;
		}
		&::-webkit-scrollbar-corner {
			all: initial !important;
		}
		&::-webkit-resizer {
			all: initial !important;
		}
	}

	.custom-scroll {
		&::-webkit-scrollbar {
			width: 1px;
		}

		&::-webkit-scrollbar-track {
			box-shadow: inset 0 0 6px rgba(0, 0, 0, 0.3);
		}

		&::-webkit-scrollbar-thumb {
			background-color: #727272;
		}
	}

	.board-container {
		display: flex;
		flex-wrap: nowrap;
		overflow-x: auto;
		overflow-y: hidden;
		@extend .reset-scroll;

		&__item {
			width: 300px;
			flex-shrink: 0;
			height: calc(100vh - 130px);
		}
	}

	.board-card {
		@extend .custom-scroll;
		overflow-x: auto;
		margin-top: 20px;
		height: calc(100% - 50px);

		div:first-child {
			margin-top: 0 !important;
		}
	}
</style>
