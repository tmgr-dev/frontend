<template>
	<teleport to="title">{{ title }}</teleport>

	<BaseLayout no-copyright>
		<template #body>
			<div class="block justify-center">
				<div class="w-full overflow-x-auto">
					<div class="min-h-15">
						<filters-board
							v-if="workspaceUsers.length"
							:workspaceUsers="workspaceUsers"
							:chosen-user.sync="chosenUser"
							@update:chosenUser="handleChosenUserUpdate"
							activeDraggable="activeDraggable"
							@handleUpdateDraggable="handleUpdateDraggable"
							@handleSearchTextChanged="handleSearchTextChanged"
						/>
					</div>
					<div class="board-container">
						<draggable
							v-if="activeDraggable"
							v-model="columns"
							group="columns"
							item-key="id"
							class="board-container"
							@end="onMove"
							data-id="column"
						>
							<template #item="{ element: column }">
								<div class="board-container__item pr-2">
									<div class="column-width h-full rounded-lg rounded px-3 pb-3">
										<div>
											<div
												class="relative flex items-center rounded pt-2 pl-2 pb-2 font-sans text-sm font-semibold tracking-wide text-tmgr-blue dark:text-tmgr-gray"
												:style="{
													'border-top': `solid 5px ${column.status.color}`,
												}"
											>
												<div class="mr-2 flex items-center hover:cursor-move">
													<svg
														width="5"
														height="8"
														viewBox="0 0 5 8"
														fill="none"
														xmlns="http://www.w3.org/2000/svg"
													>
														<circle cx="1" cy="1" r="1" fill="#D9D9D9" />
														<circle cx="4" cy="1" r="1" fill="#D9D9D9" />
														<circle cx="1" cy="4" r="1" fill="#D9D9D9" />
														<circle cx="4" cy="4" r="1" fill="#D9D9D9" />
														<circle cx="4" cy="7" r="1" fill="#D9D9D9" />
														<circle cx="1" cy="7" r="1" fill="#D9D9D9" />
													</svg>
												</div>
												{{ column.title }}
												<div class="absolute top-1/2 right-3 -translate-y-1/2">
													<dashboard-dropdown-menu
														:actions="getActions(column)"
													/>
												</div>
											</div>
										</div>

										<draggable
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
												<task-card
													:task="task"
													class="my-5 cursor-move"
													:data-task="jsonEncode(task)"
												/>
											</template>
										</draggable>
									</div>
								</div>
							</template>
						</draggable>
						<div
							v-else
							v-for="column in columns"
							:key="column.title"
							class="board-container__item pr-2"
						>
							<div class="column-width h-full rounded-lg rounded px-3 pb-3">
								<div>
									<div
										class="relative rounded pt-2 pl-2 pb-2 font-sans text-sm font-semibold tracking-wide text-tmgr-blue dark:text-tmgr-gray"
										:style="{
											'border-top': `solid 5px ${column.status.color}`,
										}"
									>
										{{ column.title }}
										<div class="absolute top-1/2 right-3 -translate-y-1/2">
											<dashboard-dropdown-menu :actions="getActions(column)" />
										</div>
									</div>
								</div>

								<draggable
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
										<task-card
											:task="task"
											class="my-5 cursor-move"
											:data-task="jsonEncode(task)"
										/>
									</template>
								</draggable>
							</div>
						</div>
						<div class="w-12 h-12 flex justify-center items-center">
							<span
								@click="isShowCreateStatusModal = true"
								class="material-icons text-2xl text-gray-500 cursor-pointer hover:text-black dark:text-gray-700 dark:hover:text-white"
								>add</span
							>
						</div>
						<Transition name="bounce-right-fade">
							<Modal
								v-if="isShowCreateStatusModal"
								modal-class="p-6 w-96"
								close-on-bg-click
								@close="isShowCreateStatusModal = false"
							>
								<template #modal-body>
									<div>
										<h1>Create status</h1>
										<label class="flex flex-col gap-2">
											Status name :
											<TextField placeholder="Name" v-model="statusName" />
										</label>
										<label class="flex flex-col gap-2">
											Status type :
											<TextField placeholder="Type" v-model="statusType" />
										</label>
										<label class="flex flex-col gap-2">
											Status color :
											<TextField placeholder="Color" v-model="statusColor" />
										</label>
										<button
											@click="createNewStatus()"
											class="mt-3 w-full rounded bg-orange-500 py-2 px-4 font-bold text-white outline-none transition hover:bg-orange-600 sm:mb-0"
											type="button"
										>
											Create
										</button>
									</div>
								</template>
							</Modal>
						</Transition>
					</div>
				</div>
			</div>
		</template>
	</BaseLayout>
</template>

<script>
	import Modal from 'src/components/Modal.vue';
	import Button from 'src/components/general/Button.vue';
	import draggable from 'vuedraggable';
	import TaskCard from 'src/components/tasks/TaskBoardCard.vue';
	import DropdownMenu from 'src/components/general/DropdownMenu.vue';
	import DashboardDropdownMenu from 'src/components/general/BoardOptionsMenu.vue';
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
	import TextField from 'src/components/general/TextField.vue';
	import {
		createStatus,
		deleteStatus,
		getStatuses,
	} from 'src/actions/tmgr/statuses';

	export default {
		name: 'Board',
		components: {
			TextField,
			Modal,
			FiltersBoard,
			DashboardDropdownMenu,
			DropdownMenu,
			Button,
			TaskCard,
			draggable,
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
			chosenUser: null,
			errors: {},
			archivedStatus: null,
			searchText: '',
			filteredTasksArray: [],
			isShowCreateStatusModal: false,
			statusName: '',
			statusType: '',
			statusColor: '',
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
		}),
		watch: {
			'$store.getters.reloadTasks'() {
				this.loadTasks();
			},
			chosenUser: function () {
				this.loadTasks();
			},
			searchText: function () {
				this.loadTasks();
			},
		},
		methods: {
			handleChosenUserUpdate(newChosenUser) {
				this.chosenUser = newChosenUser;
			},
			async createNewStatus() {
				const newStatus = {
					name: this.statusName,
					type: this.statusType,
					color: this.statusColor,
				};
				await createStatus(this.workspaceId, newStatus);
			},
			async Dlt() {
				await deleteStatus(301);
			},
			getActions(column) {
				return [
					{
						click: () => {
							this.$store.commit('showCreateTaskModal', {
								showCreateTaskModal: true,
								statusId: column.status.id,
							});
						},
						label: 'Create task',
					},
				];
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
			onStart(evt) {},
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
				if (this.chosenUser?.id) {
					const tasks = await getSortedTasksByStatus(status?.id || status, {
						params: {
							'order[column]': 'order',
							'order[direction]': 'asc',
						},
					});

					return tasks.filter((item) =>
						item.assignees.find(
							(assignee) =>
								assignee.id === this.chosenUser.id &&
								assignee.name === this.chosenUser.name,
						),
					);
				} else {
					return await getSortedTasksByStatus(status?.id || status, {
						params: {
							'order[column]': 'order',
							'order[direction]': 'asc',
						},
					});
				}
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
			},
		},
		async beforeMount() {
			const user = await getUser();
			const workspaces = await getWorkspaces();
			this.workspacesData = workspaces;
			this.userData = user;

			const workspaceSetting = this.userData.settings.find(
				(setting) => setting.key === 'current_workspace',
			);
			if (workspaceSetting) {
				this.workspaceId = +workspaceSetting.value;
				const users = await getWorkspaceMembers(this.workspaceId);
				this.workspaceUsers = [{ id: 0, name: 'Select User' }, ...users];
			}
		},
		async mounted() {
			document.body.classList.add('overflow-hidden');
			await this.loadColumns();
			await this.loadTasks();
			console.log('Statuses', await getStatuses());
		},
		unmounted() {
			document.body.classList.remove('overflow-hidden');
		},
	};
</script>

<style lang="scss" scoped>
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
