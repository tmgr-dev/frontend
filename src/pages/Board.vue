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
								@click="
									() => {
										isShowStatusModal = true;
										isCreatingStatus = true;
									}
								"
								class="material-icons text-2xl text-gray-500 cursor-pointer hover:text-black dark:text-gray-700 dark:hover:text-white"
								>add</span
							>
						</div>
						<Transition name="bounce-right-fade">
							<Modal
								v-if="isShowStatusModal"
								modal-class="p-6 w-96"
								close-on-bg-click
								@close="closeModal"
							>
								<template #modal-body>
									<div v-if="isCreatingStatus">
										<div v-if="!isShowColorPicker">
											<h1>Create status</h1>
											<button @click="Dlt">Dlt</button>
											<label class="flex flex-col gap-2">
												Status name :
												<TextField placeholder="Name" v-model="statusName" />
											</label>
											<label class="flex flex-col gap-2">
												Status type :
												<Select
													placeholder="Select Type"
													:options="statusTypes"
													v-model="statusType"
													label-key="name"
													value-key="name"
												/>
											</label>
											<label class="flex flex-col gap-2 mb-3">
												Status color :
												<button
													type="button"
													:style="{ backgroundColor: statusColor }"
													class="w-full rounded py-2 px-4 font-bold text-white outline-none transition sm:mb-0"
													:class="'bg-' + '[' + statusColor + ']'"
													@click="isShowColorPicker = true"
												>
													{{ statusColor }}
												</button>
											</label>

											<button
												@click="createNewStatus()"
												class="mt-3 w-full rounded bg-orange-500 py-2 px-4 font-bold text-white outline-none transition hover:bg-orange-600 sm:mb-0"
												type="button"
											>
												Create
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
									<div v-else>
										<div v-if="!isShowColorPicker">
											<h1>Edit status</h1>

											<label class="flex flex-col gap-2">
												Status name :
												<TextField placeholder="Name" v-model="statusName" />
											</label>
											<label class="flex flex-col gap-2">
												Status type :
												<Select
													placeholder="Select Type"
													:options="statusTypes"
													v-model="statusType"
													label-key="name"
													value-key="name"
												/>
											</label>
											<label class="flex flex-col gap-2 mb-3">
												Status color :
												<button
													type="button"
													:style="{ backgroundColor: statusColor }"
													class="w-full rounded py-2 px-4 font-bold text-white outline-none transition sm:mb-0"
													:class="'bg-' + '[' + statusColor + ']'"
													@click="isShowColorPicker = true"
												>
													{{ statusColor }}
												</button>
											</label>

											<button
												@click="editStatus()"
												class="mt-3 w-full rounded bg-orange-500 py-2 px-4 font-bold text-white outline-none transition hover:bg-orange-600 sm:mb-0"
												type="button"
											>
												Save
											</button>
											<button
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
				</div>
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
		updateStatus,
	} from 'src/actions/tmgr/statuses';
	import ColorPicker from '@radial-color-picker/vue-color-picker';
	import Select from 'src/components/general/Select.vue';
	import Confirm from 'src/components/general/Confirm.vue';

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
			chosenUser: null,
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
			onColorSelect() {
				const hexColor = this.hslToHex(
					this.color.hue,
					this.color.saturation,
					this.color.luminosity,
				);
				this.statusColor = hexColor;
				setTimeout(() => {
					this.isShowColorPicker = false;
				}, 1000);
			},
			hslToHex(h, s, l) {
				l /= 100;
				const a = (s * Math.min(l, 1 - l)) / 100;
				const f = (n) => {
					const k = (n + h / 30) % 12;
					const color = l - a * Math.max(Math.min(k - 3, 9 - k, 1), -1);
					return Math.round(255 * color)
						.toString(16)
						.padStart(2, '0');
				};
				return `#${f(0)}${f(8)}${f(4)}`;
			},
			hueFromHex(H) {
				// Convert hex to RGB first
				let r = 0,
					g = 0,
					b = 0;
				if (H.length === 4) {
					r = '0x' + H[1] + H[1];
					g = '0x' + H[2] + H[2];
					b = '0x' + H[3] + H[3];
				} else if (H.length === 7) {
					r = '0x' + H[1] + H[2];
					g = '0x' + H[3] + H[4];
					b = '0x' + H[5] + H[6];
				}
				// Then to HSL
				r /= 255;
				g /= 255;
				b /= 255;
				let cmin = Math.min(r, g, b),
					cmax = Math.max(r, g, b),
					delta = cmax - cmin,
					h = 0;

				if (delta === 0) h = 0;
				else if (cmax === r) h = ((g - b) / delta) % 6;
				else if (cmax === g) h = (b - r) / delta + 2;
				else h = (r - g) / delta + 4;

				h = Math.round(h * 60);

				if (h < 0) h += 360;

				return h;
			},
			onInput(hue) {
				this.color.hue = hue;
			},
			handleChosenUserUpdate(newChosenUser) {
				this.chosenUser = newChosenUser;
			},
			closeModal() {
				this.clearStatus();
				this.isShowStatusModal = false;
				this.isShowColorPicker = false;
			},
			clearStatus() {
				this.statusName = '';
				this.statusType = '';
				this.statusColor = '#077fe8';
			},
			async createNewStatus() {
				const newStatus = {
					name: this.statusName,
					type: this.statusType,
					color: this.statusColor,
				};

				if (this.statusName === '' && this.statusType === '') {
					alert(`'Name' and 'Type' fields are empty`);
					return;
				}
				if (this.statusName === '') {
					alert('Write the name of status');
					return;
				}
				if (this.statusType === '') {
					alert('Choose the type of status');
					return;
				} else {
					await createStatus(this.workspaceId, newStatus);
					this.showAlert('Save', 'The status was created');
					this.closeModal();
					this.$store.commit('appRerender');
				}
			},
			async editStatus() {
				const newStatusData = {
					name: this.statusName,
					type: this.statusType,
					color: this.statusColor,
				};
				if (this.statusName === '' && this.statusType === '') {
					alert(`'Name' and 'Type' fields are empty`);
					return;
				}
				if (this.statusName === '') {
					alert('Write the name of status');
					return;
				}
				if (this.statusType === '') {
					alert('Choose the type of status');
					return;
				} else {
					await updateStatus(this.statusId, newStatusData);
					this.showAlert('Saved', 'The status was edited');
					this.closeModal();
					this.$store.commit('appRerender');
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
					this.$store.commit('appRerender');
				};
				this.showConfirm(
					'Delete task',
					'Are you sure?',
					deleteStatusConfirmation,
				);
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
					{
						click: () => {
							this.isShowStatusModal = true;
							this.statusType = column.status.type;
							this.statusName = column.status.name;
							this.statusColor = column.status.color;
							this.statusId = column.status.id;
							this.color.hue = this.hueFromHex(this.statusColor);
						},
						label: 'Edit status',
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
			this.color.hue = this.hueFromHex(this.statusColor);
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
