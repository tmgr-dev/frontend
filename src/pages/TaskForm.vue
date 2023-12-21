<template>
	<teleport to="title">{{ form.title || h1.main }}&nbsp;</teleport>

	<div class="items-between text-center sm:flex">
		<div ref="editing_task_category" v-if="!isCreatingTask">
			<!--	Settings modal		-->
			<Transition name="bounce-right-fade">
				<Modal
					id="modal3"
					v-if="isShowSettingsModal"
					modal-class="p-6 w-96"
					@closingModal="closingModal"
				>
					<template #modal-body>
						<form
							@submit.prevent="updateCategory"
							class="text-gray-800 dark:text-tmgr-gray"
						>
							<label for="settings" class="mb-5 block text-lg font-bold">
								Settings
							</label>

							<div>
								<div
									v-if="form.user"
									class="estimated-info border-b border-neutral-200 py-1 pr-5 text-start dark:border-neutral-600"
								>
									Author:
									<span class="text-neutral-600 dark:text-neutral-300">
										{{ form.user.name }}
									</span>
								</div>

								<label class="mb-2 mt-4 block text-left text-sm font-bold">
									<span class="mb-2 block">Category</span>

									<Select
										:options="categoriesSelectOptions"
										label-key="title"
										value-key="id"
										v-model="currentCategoryOptionInSelect"
									/>
								</label>

								<div
									v-for="(setting, index) in availableSettings"
									id="settings"
									class="mt-4"
								>
									<label
										:for="`setting-${setting.id}`"
										class="mb-2 block text-left text-sm font-bold"
									>
										{{ setting.name }}
									</label>

									<div class="relative mb-4">
										<TimeField
											v-if="setting.component_type === 'time_in_seconds'"
											v-model="settings[index].value"
											:placeholder="setting.description"
										/>

										<TextField
											v-else
											v-model="settings[index].value"
											:placeholder="setting.description"
										/>
									</div>
								</div>

								<div class="estimated-info py-2 pr-5 text-start">
									<div
										class="flex items-center border-b border-neutral-200 dark:border-neutral-600"
									>
										Assignees
										<span
											class="material-icons checkpoint-delete text-lg"
											@click="
												(isShowModalAssign = true), $store.commit('openModal')
											"
										>
											add
										</span>
									</div>

									<div
										v-if="form.assignees && form.assignees.length"
										v-for="assignee in form.assignees"
										class="mt-2 flex gap-2"
									>
										<div class="flex items-center gap-1.5">
											{{ assignee.name }}

											<span
												class="material-icons checkpoint-delete text-lg text-red-500"
												@click="deleteAssignee(assignee.id)"
											>
												person_remove
											</span>
										</div>
									</div>
								</div>
							</div>

							<div class="mt-6 flex flex-nowrap items-center">
								<button
									type="button"
									@click="closingModal"
									class="mr-1 block w-2/4 rounded bg-gray-700 p-2 text-white"
								>
									Cancel
								</button>

								<button
									type="submit"
									class="mr-1 block w-2/4 rounded bg-blue-700 p-2 text-white"
								>
									Update
								</button>
							</div>
						</form>
					</template>
				</Modal>
			</Transition>

			<!-- assigners -->
			<Transition name="bounce-right-fade">
				<Modal
					id="modal4"
					v-if="isShowModalAssign"
					modal-class="p-6 w-96"
					@closingModal="closingModal"
				>
					<template #modal-body>
						<form @submit.prevent="" class="text-gray-800 dark:text-tmgr-gray">
							<label for="settings" class="block text-lg font-bold">
								Assign task to user
							</label>

							<div class="mt-5 grid grid-cols-2 gap-2">
								<div
									v-for="workspaceMember in workspaceMembers"
									:key="workspaceMember.id"
								>
									<div
										class="group flex cursor-pointer items-center gap-1.5 dark:hover:text-white"
										@click="handleAssign(workspaceMember.id)"
									>
										<span
											class="material-icons text-lg text-neutral-300 dark:group-hover:text-white"
										>
											<template
												v-if="
													form.assignees.find(
														(user) => user.id === workspaceMember.id,
													)
												"
											>
												radio_button_checked
											</template>

											<template v-else>radio_button_unchecked</template>
										</span>

										<span>{{ workspaceMember.name }}</span>
									</div>
								</div>
							</div>

							<div class="mt-6 flex flex-nowrap items-center">
								<button
									type="button"
									@click="closingModal"
									class="mr-1 block w-full rounded bg-gray-700 p-2 text-white"
								>
									Close
								</button>
							</div>
						</form>
					</template>
				</Modal>
			</Transition>
		</div>
	</div>

	<div
		ref="modal"
		:class="{
			'task-form-container relative mx-auto  overflow-hidden rounded-lg bg-white  p-3 dark:bg-gray-900':
				isModal,
			'container mx-auto p-3': !isModal,
		}"
	>
		<header ref="header">
			<div
				:class="`flex  justify-between md:items-center ${
					isModal ? '' : 'mt-10'
				}`"
			>
				<template v-if="!isCreatingTask">
					<router-link
						:to="
							!currentCategory
								? '/'
								: `/projects-categories/${
										currentCategory.id
								  }/children/${getCategoryStatus()}`
						"
						class="hidden rounded text-blue-800 text-white focus:outline-none dark:text-neutral-200 sm:mb-0 sm:block"
						type="button"
					>
						{{ currentCategory ? currentCategory.title : 'tasks' }}
					</router-link>
					<div class="md:flex">
						<Select
							v-model="form.status_id"
							:options="workspaceStatuses"
							label-key="name"
							value-key="id"
							class="w-36 shrink-0 sm:ml-3 sm:w-40"
						/>
						<div
							v-if="categoriesSelectOptions.length >= 2"
							class="mt-2 w-36 md:m-0 md:ml-3 md:mr-3"
						>
							<Select
								v-if="!isCreatingTask"
								placeholder="Select category"
								:options="categoriesSelectOptions"
								v-model="form.project_category_id"
								label-key="title"
								value-key="id"
							/>
						</div>
					</div>

					<assignee-users
						:assignees="form.assignees"
						:is-modal="isModal"
						avatarsClass="h-8 w-8"
						:show-assignee-controls="true"
						@showModal="(isShowModalAssign = true), $store.commit('openModal')"
						@deleteAssignee="deleteAssignee"
					/>
				</template>

				<p v-else>Creating task</p>
				<div
					v-if="categoriesSelectOptions.length >= 2"
					class="mt-2 md:m-0 md:ml-3 md:mr-3 md:w-48"
				>
					<Select
						v-if="isCreatingTask"
						placeholder="Select category"
						:options="categoriesSelectOptions"
						v-model="form.project_category_id"
						label-key="title"
						value-key="id"
					/>
				</div>

				<div v-if="isModal" class="ml-auto gap-2 md:flex">
					<button
						type="button"
						class="mr-2 hidden opacity-50 transition-opacity hover:opacity-100 sm:block"
					>
						<a
							class="material-icons text-2xl text-black dark:text-white"
							:href="`/${taskId}`"
						>
							open_in_new
						</a>
					</button>

					<button
						type="button"
						class="opacity-50 transition-opacity hover:opacity-100"
						v-if="isModal"
					>
						<span
							class="material-icons text-2xl text-black dark:text-white"
							@click="close"
						>
							close
						</span>
					</button>
				</div>
			</div>
		</header>
		<div class="h-full justify-center overflow-y-scroll md:flex">
			<section
				role="main"
				class="text-center md:w-1/2"
				:class="{ 'mt-10': !form.start_time }"
			>
				<Transition>
					<div class="mt-8 text-center" :key="this.form.common_time">
						<Countdown
							v-if="form.id"
							:init-task="form"
							@toggle="toggleCountdown"
							@update:seconds="updateSeconds"
						/>

						<Countdown
							v-else
							disabled
							:init-task="form"
							@update:seconds="updateSeconds"
						/>
					</div>
				</Transition>
				<div class="mb-5">
					<TextField
						v-model="form.title"
						:errors="errors.title"
						placeholder="Task name"
					/>

					<quill-editor
						class="relative z-10 !mt-2 min-h-[200px] rounded bg-white py-2 px-3 outline-none transition-colors duration-300 dark:bg-gray-800"
						:class="errors.description && 'border-red-500'"
						v-model:content="form.description"
						content-type="html"
						theme="bubble"
						placeholder="Description"
					/>
					<div v-if="form.description?.length" class="my-2 block w-full">
						<div
							class="float-right cursor-pointer opacity-50 hover:opacity-100"
						>
							<svg
								v-if="isDescriptionOptimizing"
								class="h-5 w-5 animate-spin"
								xmlns="http://www.w3.org/2000/svg"
								fill="none"
								viewBox="0 0 24 24"
							>
								<circle
									class="opacity-25"
									cx="12"
									cy="12"
									r="10"
									stroke="currentColor"
									stroke-width="4"
								/>
								<path
									class="opacity-75"
									fill="currentColor"
									d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
								/>
							</svg>
							<svg
								@click="optimizeWithAI"
								v-else
								xmlns="http://www.w3.org/2000/svg"
								viewBox="0 0 24 24"
								fill="currentColor"
								class="h-6 w-6"
							>
								<path
									fill-rule="evenodd"
									d="M9 4.5a.75.75 0 01.721.544l.813 2.846a3.75 3.75 0 002.576 2.576l2.846.813a.75.75 0 010 1.442l-2.846.813a3.75 3.75 0 00-2.576 2.576l-.813 2.846a.75.75 0 01-1.442 0l-.813-2.846a3.75 3.75 0 00-2.576-2.576l-2.846-.813a.75.75 0 010-1.442l2.846-.813A3.75 3.75 0 007.466 7.89l.813-2.846A.75.75 0 019 4.5zM18 1.5a.75.75 0 01.728.568l.258 1.036c.236.94.97 1.674 1.91 1.91l1.036.258a.75.75 0 010 1.456l-1.036.258c-.94.236-1.674.97-1.91 1.91l-.258 1.036a.75.75 0 01-1.456 0l-.258-1.036a2.625 2.625 0 00-1.91-1.91l-1.036-.258a.75.75 0 010-1.456l1.036-.258a2.625 2.625 0 001.91-1.91l.258-1.036A.75.75 0 0118 1.5zM16.5 15a.75.75 0 01.712.513l.394 1.183c.15.447.5.799.948.948l1.183.395a.75.75 0 010 1.422l-1.183.395c-.447.15-.799.5-.948.948l-.395 1.183a.75.75 0 01-1.422 0l-.395-1.183a1.5 1.5 0 00-.948-.948l-1.183-.395a.75.75 0 010-1.422l1.183-.395c.447-.15.799-.5.948-.948l.395-1.183A.75.75 0 0116.5 15z"
									clip-rule="evenodd"
								/>
							</svg>
						</div>
					</div>
				</div>

				<div
					v-if="!isCreatingTask"
					class="checkpoints-wrapper rounded"
					:key="checkpointUpdateKey"
				>
					<div class="text-bold flex items-center justify-center gap-2 text-sm">
						{{
							form.checkpoints.length
								? 'Add a checkpoint'
								: 'Create checkpoints'
						}}
						<span
							class="material-icons checkpoint-delete text-lg text-gray-500"
							@click="addCheckpoint"
						>
							add
						</span>
					</div>
					<checkpoints :checkpoints="form.checkpoints" />
				</div>
			</section>
			<section v-if="!isCreatingTask" class="mt-10 md:w-1/2">
				<comments-chat
					:workspaceMembers="workspaceMembers"
					:assignees="form.assignees"
					:taskId="taskId"
					:startTime="form.start_time"
					:isDataEdited="isDataEdited"
					ref="commentsChat"
				/>
			</section>
		</div>

		<footer
			ref="footer"
			class="shadow-top z-10 mt-10 w-full rounded-lg p-2 sm:p-5"
			:class="{ 'mt-30': isPage }"
		>
			<task-actions
				:is-creating-task="isCreatingTask"
				:is-data-edited="isDataEdited"
				:is-saving="isSaving"
				@removeTask="removeTask(form)"
				@createTask="createTask"
				@saveTask="saveTask"
				@settingsTask="showModalCategory"
				@cancelCreateTask="cancelCreateTask"
			>
				<span
					v-if="form.approximately_time"
					:class="`text-${
						approximatelyEndTime === '00:00' ? 'red' : 'gray'
					}-500 estimated-info hidden py-2 pr-5 md:block`"
				>
					Left time: {{ approximatelyEndTime }}
				</span>
			</task-actions>
		</footer>
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

<script>
	import TaskActions from 'src/components/tasks/TaskActions.vue';
	import Countdown from 'src/components/general/Countdown.vue';
	import Confirm from 'src/components/general/Confirm.vue';
	import {
		deleteTask,
		getTask,
		createTask as createTaskAction,
		updateTask,
		stopTaskTimeCounter,
		startTaskTimeCounter,
		addTaskAssignee,
		deleteTaskAssignee,
		getTasksIndexes,
		optimizeWithAI,
	} from 'src/actions/tmgr/tasks';
	import {
		getTaskSettings,
		updateOneTaskSettings,
	} from 'src/actions/tmgr/settings';
	import { getCategories, getCategory } from 'src/actions/tmgr/categories';
	import { getWorkspaceMembers } from 'src/actions/tmgr/workspaces';
	import Select from 'src/components/general/Select.vue';
	import Switcher from 'src/components/general/Switcher.vue';
	import TextField from 'src/components/general/TextField.vue';
	import TimeField from 'src/components/general/TimeField.vue';
	import AssigneeUsers from 'src/components/general/AssigneeUsers.vue';
	import { titlePatternHandler } from 'src/utils/titlePatternHandler';
	import Modal from 'src/components/Modal.vue';
	import { mapState } from 'vuex';
	import CommentsChat from 'src/components/general/CommentsChat.vue';
	import store from 'src/store';
	import Checkpoints from 'src/components/general/Checkpoints.vue';

	export default {
		name: 'TaskForm',
		components: {
			Checkpoints,
			CommentsChat,
			Modal,
			AssigneeUsers,
			TimeField,
			TextField,
			Switcher,
			Select,
			Confirm,
			Countdown,
			TaskActions,
		},
		props: {
			isModal: {
				required: false,
				type: Boolean,
				default: false,
			},
			modalTaskId: {
				required: false,
				type: Number,
				default: null,
			},
			statusId: {
				required: false,
				type: Number,
				default: null,
			},
			modalProjectCategoryId: {
				required: false,
				type: Number,
				default: null,
			},
		},
		emits: ['close', 'updated'],
		data() {
			return {
				isDescriptionOptimizing: false,
				isActiveAssignBtns: true,
				middleBlockHeight: null,
				confirm: null,
				savedData: {},
				availableSettings: [],
				settings: [],
				isSaving: false,
				autoSaveTimeout: null,
				workspaceMembers: [],
				watchingFields: [
					'title',
					'description',
					'status_id',
					'project_category_id',
					'checkpoints',
					'approximately_time',
					'settings',
				],
				errors: {},
				showEditDescription: false,
				panel: false,
				isOpen: false,
				checkpointUpdateKey: 0,
				counter: {
					common_time: 0,
					start_time: 0,
				},
				statuses: [
					{ value: 'created', name: `Created` },
					{ value: 'active', name: `Active` },
					{ value: 'done', name: `Done` },
				],
				h1: {
					main: `${this.taskId ? 'Edit' : 'Create'} task`,
					CurrentTasksList: 'Current tasks',
					HiddenTasksList: 'Hidden tasks',
					ArchiveTasksList: 'Archive tasks',
				},
				selected: false,
				form: {
					title: '',
					status: 'created',
					status_id: this.statusId,
					project_category_id: this.projectCategoryId || '',
					description: '',
					common_time: 0,
					checkpoints: [],
				},
				isShowSettingsModal: false,
				isShowModalAssign: false,
				categoriesSelectOptions: [],
				currentCategory: '',
				approximatelyTime: null,
				currentCategoryOptionInSelect: null,
				prevValue: null,
				edited: false,
				isProcessing: false,
				comment: {},
				isShowAlert: true,
			};
		},
		watch: {
			form(newVal) {
				this.setSavedData(newVal);
			},
			'form.project_category_id': async function (newVal, oldVal) {
				this.currentCategory = await getCategory(newVal);
				this.loadCategory();
			},
		},

		async mounted() {
			this.$store.commit('resetOpenModals');

			this.handleHistoryState();
			if (this.categoriesSelectOptions.length === 0) {
				await this.loadCategories();
			}
		},
		unmounted() {
			this.$store.commit('closeTaskModal');
		},
		computed: {
			store() {
				return store;
			},
			...mapState(['workspaceStatuses']),
			taskId() {
				return this.projectCategoryId
					? null
					: this.form?.id || this.modalTaskId || this.$route.params.id;
			},
			approximatelyEndTime() {
				const secondsLeft =
					new Date().getSeconds() +
					(this.form.approximately_time - this.form.common_time);

				return this.toHHMM(secondsLeft < 0 ? 0 : secondsLeft);
			},
			isCreatingTask() {
				return !this.taskId;
			},
			projectCategoryId() {
				return this.form?.id
					? null
					: this.$route.params.project_category_id ||
							this.modalProjectCategoryId;
			},
			isDataEdited() {
				if (!this.form.id) {
					return false;
				}
				for (let i = 0; i < this.watchingFields.length; ++i) {
					const field = this.watchingFields[i];

					if (field === 'checkpoints') {
						const first = this.removeFieldsFromArray(this.savedData[field], [
							'end',
						]);
						const second = this.removeFieldsFromArray(this.form[field], [
							'end',
						]);
						if (!this.equals(first, second)) {
							if (this.equals(this.prevValue, second)) {
								return true;
							}
							this.prevValue = JSON.parse(JSON.stringify(second));
							this.dispatchAutoSave();
							return true;
						}
					} else if (!this.equals(this.savedData[field], this.form[field])) {
						this.dispatchAutoSave();
						return true;
					}
				}
				return false;
			},
			isPage() {
				return (
					this.$route.name === 'TasksEdit' || this.$route.name === 'TasksCreate'
				);
			},
		},
		methods: {
			async optimizeWithAI() {
				this.isDescriptionOptimizing = true;
				const result = await optimizeWithAI(
					`${this.form.title}: ${this.form.description.replace(
						/(<([^>]+)>)/gi,
						'',
					)}`,
				);
				this.form.description = result;

				this.isDescriptionOptimizing = false;
			},
			handleHistoryState() {
				if (this.isModal && !this.isCreatingTask) {
					history.pushState({}, '', `/${this.taskId}`);
				}
			},
			close() {
				this.$emit('close');
				this.$store.commit('incrementReloadTasksKey');
			},

			closingModal() {
				if (this.isShowModalAssign) {
					this.isShowModalAssign = false;
					this.$store.commit('closeModal');
					return;
				}
				if (this.isShowSettingsModal && !this.isShowModalAssign) {
					this.isShowSettingsModal = false;
					this.$store.commit('closeModal');
					return;
				}
				if (!this.isShowSettingsModal && !this.isShowModalAssign) {
					this.cancelCreateTask();
				}
			},
			showConfirm(title, body, action) {
				this.confirm = { title, body, action };
			},
			async removeTask(task) {
				if (this.form.start_time) {
					this.form = await stopTaskTimeCounter(this.taskId);
				}
				const deleteTaskConfirmation = async () => {
					try {
						task.deleted_at = await deleteTask(this.taskId);

						if (this.isPage) {
							this.$router.replace('/');
						}
					} catch (e) {
						console.error(e);
					} finally {
						this.confirm = null;
						this.close();
					}
				};
				this.showConfirm(
					'Delete task',
					'Are you sure?',
					deleteTaskConfirmation,
				);
			},
			async loadTaskSettings() {
				const data = await getTaskSettings();
				this.initSettings(data, this.form.settings);
				this.availableSettings = data;
			},
			initSettings(availableSettings, settings = []) {
				return availableSettings.map((item, index) => {
					const setting = this.getSettingById(settings, item.id, {
						id: item.id,
						value: '',
					});
					this.settings[index] = setting;
					item.show_custom_value_input =
						item.default_values &&
						item.default_values.findIndex(
							(val) => val.value === setting.value,
						) === -1;
				});
			},
			getSettingById(settings, id, defaultResult = null) {
				return settings.find((setting) => setting.id === id) || defaultResult;
			},
			async saveSettings(settings) {
				const data = await updateOneTaskSettings(this.form.id, settings);
				this.initSettings(this.availableSettings, data.settings);
			},
			async handleAssign(userId) {
				const isAssigned = this.form.assignees.find(
					(user) => user.id === userId,
				);

				if (isAssigned) {
					await this.deleteAssignee(userId);
				} else {
					await this.addAssign(userId);
				}
			},
			async addAssign(userId) {
				this.form.assignees = await addTaskAssignee(this.form.id, userId);
			},
			async deleteAssignee(userId) {
				this.form.assignees = await deleteTaskAssignee(this.form.id, userId);
			},
			setFormDataWithDelay(data, delay = 200) {
				return new Promise((resolve, reject) => {
					if (this.form.id === data.id) {
						this.form.id = null;
						setTimeout(() => {
							this.form = data;
							resolve();
						}, delay);
					}
				});
			},
			async dispatchAutoSave() {
				this.removeDispatchedAutoSave();
				this.isShowAlert = false;
				this.autoSaveTimeout = setTimeout(this.saveTask, 5000);
			},
			removeDispatchedAutoSave() {
				if (this.autoSaveTimeout) {
					clearTimeout(this.autoSaveTimeout);
				}
			},
			equals(o1, o2) {
				return JSON.stringify(o1) === JSON.stringify(o2);
			},
			removeFieldsFromArray(arr, fieldsForDelete) {
				if (!arr) {
					return;
				}
				arr = JSON.parse(JSON.stringify(arr));
				return arr.map((item) => {
					const keys = Object.keys(item).filter(
						(key) => !fieldsForDelete.includes(key),
					);
					const result = {};
					keys.forEach((key) => (result[key] = item[key]));
					return result;
				});
			},
			async showModalCategory() {
				try {
					this.isShowSettingsModal = true;
					this.$store.commit('openModal');
					if (this.categoriesSelectOptions.length === 0) {
						await this.loadCategories();
					}
				} catch (e) {
					console.error(e);
				}
			},
			async updateCategory() {
				if (this.currentCategoryOptionInSelect) {
					this.form.project_category_id = this.currentCategoryOptionInSelect;
				}
				this.isShowSettingsModal = false;
				this.$store.commit('closeModal');
				this.isShowAlert = true;
				await this.saveTask();
				await this.loadCategory();
			},
			getShortcutSaveListener() {
				return (e) => {
					if (
						(e.metaKey || e.ctrlKey) &&
						(e.key.toLowerCase() === 's' || e.key.toLowerCase() === 'ы')
					) {
						e.preventDefault();
						this.isShowAlert = true;
						this.saveTask();
					}
				};
			},
			getActions() {
				const actions = [
					{
						click: () => {
							this[this.form.id ? 'save' : 'create']();
						},
						label: this.form.id ? 'Save' : 'Create',
					},
				];
				actions.push({
					click: () => {
						this[this.isCreatingTask ? 'cancel' : 'goToCurrentTasks']();
					},
					label: this.isCreatingTask ? 'Cancel' : 'tasks',
				});
				return actions;
			},
			async loadCategories() {
				const data = await getCategories();
				this.categoriesSelectOptions = [
					{ id: null, title: 'Without category' },
					...data,
				];
			},
			async loadCategory() {
				if (this.projectCategoryId || this.form.project_category_id) {
					this.currentCategory = await getCategory(
						+this.projectCategoryId || +this.form.project_category_id,
					);

					this.currentCategoryOptionInSelect = this.currentCategory.id;

					if (!!this.form.id || this.currentCategory.settings.length === 0)
						return;

					for (const setting of this.currentCategory.settings) {
						if (setting.key === 'task_name_pattern_date&time') {
							const indexes = await getTasksIndexes(this.currentCategory.id);
							for (const setting of this.currentCategory.settings) {
								if (setting.key === 'task_name_pattern_date&time') {
									const indexes = await getTasksIndexes(
										this.currentCategory.id,
									);
									const category = titlePatternHandler(
										setting.value,
										new Map(Object.entries(indexes)),
									);
									if (this.form.title) {
										if (this.form.title.includes(':')) {
											const cuttedTitle = this.form.title.split(':')[1].trim();
											this.form.title = `${category} ${cuttedTitle}`;
										} else {
											this.form.title = `${category} ${this.form.title}`;
										}
									} else {
										this.form.title = category;
									}
								}
							}
							if (setting.key === 'approximately_time') {
								this.form.approximately_time = parseInt(setting.value);
							}
						}
					}
				}
			},
			getTaskSettingValue(key) {
				return this.form.settings?.find((item) => item.key === key)?.value;
			},
			async loadModel() {
				try {
					this.form = await getTask(this.taskId);
				} catch (e) {
					if (e.response?.status === 404) {
						// @todo show error alert
						//this.showAlert();

						if (this.isPage) {
							this.$router.replace('/');
						}
					}
				}
			},
			setSavedData(data) {
				this.watchingFields.forEach(
					(field) =>
						(this.savedData[field] = JSON.parse(JSON.stringify(data[field]))),
				);
			},
			toHHMM(seconds) {
				let hours = Math.floor(seconds / 3600);
				let minutes = Math.floor((seconds - hours * 3600) / 60);

				if (hours < 10) {
					hours = '0' + hours;
				}
				if (minutes < 10) {
					minutes = '0' + minutes;
				}
				return hours + ':' + minutes;
			},
			getCategoryStatus() {
				if (this.form.status === 'created' || this.form.status === 'active') {
					return 'current';
				}
				return this.form.status;
			},
			async toggleCountdown() {
				this.isShowAlert = false;

				if (this.form.start_time) {
					this.form = await stopTaskTimeCounter(this.taskId);
				} else {
					this.form = await startTaskTimeCounter(this.taskId);
				}
				this.updateSeconds(this.form.common_time);

				if (this.form.start_time && this.form.status_id) {
					const statusCurrent = this.workspaceStatuses.find(
						(el) => el.type !== 'active',
					);
					if (statusCurrent) {
						const firstActiveStatus = this.workspaceStatuses.find(
							(el) => el.type === 'active',
						);
						if (firstActiveStatus) {
							this.form.status_id = firstActiveStatus.id;
						}
					}
				}
				await this.saveTask();
			},
			prepareForm() {
				if (this.form.project_category_id === '') {
					delete this.form.project_category_id;
				}
				if (!this.isCreatingTask) {
					this.form.approximately_time =
						this.getTaskSettingValue('approximately_time');
				}
			},
			async createTask() {
				try {
					this.prepareForm();
					const data = await createTaskAction(this.form);
					this.$emit('updated');
					this.$store.commit('incrementReloadTasksKey');

					if (!this.isCreatingTask) {
						this.showAlert();
					}
					if (!this.isModal) {
						await this.$router.push({
							name: 'TasksEdit',
							params: {
								id: data.id,
							},
						});
					} else {
						this.form = data;
						this.handleHistoryState();
						await this.initComponent();
					}
				} catch (e) {
					if (e.response && e.response && e.response.data.errors) {
						this.errors = e.response.data.errors;
					}
				}
			},
			cancelCreateTask() {
				if (this.isCreatingTask && (this.form.title || this.form.description)) {
					this.showConfirm('Cancel task', 'Are you sure?', () => {
						this.$emit('close');
					});
				} else {
					this.isShowAlert = true;
					this.saveTask();
					this.$emit('close');
				}
			},
			async saveTask(start = false) {
				try {
					this.isSaving = true;
					this.prepareForm();
					const data = await updateTask(this.taskId, this.form);
					this.$emit('updated');
					this.$store.commit('incrementReloadTasksKey');

					if (data.approximately_time) {
						this.approximatelyTime = this.toHHMM(data.approximately_time);
					}
					this.form = data;

					await this.saveSettings(this.settings);
					if (this.isShowAlert) {
						this.showAlert('Saved', 'The task was saved');
					}

					this.removeDispatchedAutoSave();
					const id = this.form.id;
					this.form.id = null;
					setTimeout(() => (this.form.id = id), 1);
				} catch (e) {
					if (e.response && e.response && e.response.data.errors) {
						this.errors = e.response.data.errors;
					}
				} finally {
					this.isSaving = false;
					setTimeout(() => {
						if (start && !this.form.start_time) {
							this.toggleCountdown();
						}
					}, 500);
				}
			},
			goToCurrentTasks() {
				this.$router.push('/');
			},
			addCheckpoint() {
				const { form } = this;
				if (!form.checkpoints) {
					form.checkpoints = [];
				}
				const timeSinceStartTime = !form.start_time
					? 0
					: Math.floor(
							(new Date() - new Date().setTime(form.start_time * 1000)) / 1000,
					  );
				const currentTime = form.common_time + timeSinceStartTime;
				if (form.checkpoints.length > 0) {
					const prevCheckpointIndex = form.checkpoints.length - 1;
					form.checkpoints[prevCheckpointIndex].end = currentTime;
				}
				form.checkpoints.push({
					description: 'New one',
					start: currentTime,
					end: currentTime,
				});
				++this.checkpointUpdateKey;
			},
			updateSeconds(seconds) {
				if (!this.form.checkpoints || this.form.checkpoints.length === 0) {
					return;
				}
				this.form.checkpoints[this.form.checkpoints.length - 1].end = seconds;
			},
			async initComponent() {
				if (this.taskId) {
					await this.loadModel();
					this.workspaceMembers = await getWorkspaceMembers(
						this.form.workspace_id,
					);
					window.onkeydown = this.getShortcutSaveListener();
				}

				if (this.projectCategoryId && this.isCreatingTask) {
					this.form.project_category_id = this.projectCategoryId;
				}
				await this.loadCategory();
				await this.loadTaskSettings();
				this.$store.getters.getPusher
					.private(`App.Workspace.${this.form.workspace_id}`)
					.on('comment-added', ({ comment }) => {
						if (this.taskId === comment.task_id) {
							this.$refs.commentsChat.addingComment(comment);
						}
					})
					.on('comment-updated', ({ comment }) => {
						if (this.taskId === comment.task_id) {
							this.$refs.commentsChat.editingComment(comment);
						}
					})
					.on('comment-deleted', ({ comment }) => {
						if (this.taskId === comment.task_id) {
							this.$refs.commentsChat.deletingComment(comment);
						}
					});

				this.$store.getters.getPusher
					.private(`App.User.${this.$store.state.user.id}`)
					.on('task-countdown-stopped', ({ task }) => {
						const isCountdownStarted = !!this.form.start_time;
						if (!isCountdownStarted) {
							return;
						}

						this.setFormDataWithDelay(task);
					})
					.on('task-countdown-started', ({ task }) => {
						const isCountdownStarted = !!this.form.start_time;
						if (!isCountdownStarted) {
							this.setFormDataWithDelay(task);
						}
					});
			},
		},
		async created() {
			await this.initComponent();
		},
	};
</script>

<style lang="scss" scoped>
	.task-title-span {
		max-width: 200px;
		display: inline-block;
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
		top: 16px;
		position: relative;
	}
	.shadow-top {
		--tw-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1),
			0 2px 4px -1px rgba(0, 0, 0, 0.06);
		box-shadow: 0 -4px 6px -1px rgba(0, 0, 0, 0.16);
	}

	.container {
		@media (max-width: 768px) {
			max-width: 100% !important;
		}
	}
</style>
