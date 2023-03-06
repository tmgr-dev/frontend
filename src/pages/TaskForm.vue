<template>
	<teleport to="title"> {{ form.title || h1.main }}&nbsp; </teleport>

	<div class="items-between text-center sm:flex">
		<div ref="editing_task_category" v-if="!isCreatingTask">
			<!--	Settings modal		-->
			<Transition name="bounce-right-fade">
				<modal
					v-if="isShowSettingsModal"
					modal-class="p-6 w-96"
					@close="isShowSettingsModal = false"
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
											@click="isShowModalAssign = true"
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
									@click="isShowSettingsModal = false"
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
				</modal>
			</Transition>

			<!-- assigners -->
			<Transition name="bounce-right-fade">
				<modal
					v-if="isShowModalAssign"
					modal-class="p-6 w-96"
					@close="isShowModalAssign = false"
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
									@click="isShowModalAssign = false"
									class="mr-1 block w-full rounded bg-gray-700 p-2 text-white"
								>
									Close
								</button>
							</div>
						</form>
					</template>
				</modal>
			</Transition>
		</div>
	</div>

	<div
		ref="modal"
		:class="{
			'task-form-container relative mx-auto overflow-hidden rounded-lg bg-white p-3 dark:bg-gray-900':
				isModal,
			'container mx-auto': !isModal,
		}"
	>
		<header ref="header">
			<div
				:class="`flex items-center justify-between ${isModal ? '' : 'mt-10'}`"
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

					<Select
						v-model="form.status_id"
						:options="workspaceStatuses"
						label-key="name"
						value-key="id"
						class="w-36 shrink-0 sm:ml-3 sm:w-40"
					/>

					<assignee-users
						:assignees="form.assignees"
						avatarsClass="h-8 w-8"
						:show-assignee-controls="true"
						@showModal="isShowModalAssign = true"
						@deleteAssignee="deleteAssignee"
					/>
				</template>

				<p v-else>Creating task</p>

				<div v-if="isModal" class="ml-auto flex gap-2">
					<button
						type="button"
						class="mr-2 hidden opacity-50 transition-opacity hover:opacity-100 sm:block"
					>
						<router-link
							class="material-icons text-2xl text-black dark:text-white"
							:to="`/${taskId}/edit`"
						>
							open_in_new
						</router-link>
					</button>

					<button
						type="button"
						class="opacity-50 transition-opacity hover:opacity-100"
						v-if="isModal"
					>
						<span
							class="material-icons text-2xl text-black dark:text-white"
							@click="$emit('close')"
						>
							close
						</span>
					</button>
				</div>
			</div>

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
		</header>

		<section role="main" class="mt-10 text-center">
			<div class="mb-5">
				<TextField
					v-model="form.title"
					:errors="errors.title"
					placeholder="Task name"
				/>

				<quill-editor
					class="relative z-10 !mt-2 min-h-[100px] rounded bg-white py-2 px-3 outline-none transition-colors duration-300 dark:bg-gray-800"
					:class="errors.description && 'border-red-500'"
					v-model:content="form.description"
					content-type="html"
					theme="bubble"
					placeholder="Description"
				/>
			</div>

			<div
				v-if="!isCreatingTask"
				class="checkpoints-wrapper rounded"
				:key="checkpointUpdateKey"
			>
				<div class="text-bold flex items-center justify-center gap-2 text-sm">
					{{
						form.checkpoints.length ? 'Add a checkpoint' : 'Create checkpoints'
					}}
					<span
						class="material-icons checkpoint-delete text-lg text-gray-500"
						@click="addCheckpoint"
					>
						add
					</span>
				</div>

				<div
					v-for="(checkpoint, v) in form.checkpoints"
					:key="v"
					class="mb-1 flex"
				>
					<div class="relative w-full">
						<span :class="`absolute left-0 top-0 z-10 mt-1.5 ml-1.5`">
							{{ v + 1 }}
						</span>

						<textarea
							class="max-h-40 min-h-[36px] w-full rounded bg-white py-2 px-3 pr-44 pt-2 pl-7 leading-tight outline-none transition-colors duration-300 dark:bg-gray-800"
							:class="[checkpoint.inputType === 'text' ? 'h-9' : '']"
							placeholder="Checkpoint content"
							v-model="checkpoint.description"
						/>

						<span
							class="absolute right-0 top-2 flex items-center gap-1 text-sm"
						>
							<span class="text-sm">
								{{ secondsToStringTime(checkpoint.start) }} -
								{{ secondsToStringTime(checkpoint.end) }}
							</span>

							<span
								class="material-icons checkpoint-delete text-base leading-none text-blue-700"
								@click="changeCheckpointInputField(v)"
							>
								edit
							</span>

							<span
								class="material-icons checkpoint-delete text-base leading-none text-red-700"
								@click="deleteCheckpoint(v)"
							>
								delete
							</span>
						</span>
					</div>
				</div>
			</div>
		</section>

		<footer
			ref="footer"
			class="shadow-top z-10 w-full rounded-lg p-2 sm:p-5"
			:class="{ 'mt-10': isPage }"
		>
			<task-actions
				:is-creating-task="isCreatingTask"
				:is-data-edited="isDataEdited"
				:is-saving="isSaving"
				@removeTask="removeTask(form)"
				@createTask="createTask"
				@saveTask="saveTask"
				@settingsTask="showModalCategory"
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
	import moment from 'moment';
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

	export default {
		name: 'TaskForm',
		components: {
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
			};
		},
		watch: {
			form(newVal) {
				this.setSavedData(newVal);
			},
		},
		unmounted() {
			this.$store.dispatch('closeTaskModal');
		},
		computed: {
			userSettings() {
				return this.$store.getters.getUserSettings ?? {};
			},
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
			workspaceStatuses() {
				return this.$store.getters.statuses;
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
			close() {
				this.$emit('close');
				this.$store.dispatch('reloadTasks');
			},
			showConfirm(title, body, action) {
				this.confirm = { title, body, action };
			},
			async removeTask(task) {
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
			dispatchAutoSave() {
				this.removeDispatchedAutoSave();
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
				await this.saveTask();
				await this.loadCategory();
			},
			getShortcutSaveListener() {
				return (e) => {
					if (
						e.ctrlKey &&
						(e.key.toLowerCase() === 's' || e.key.toLowerCase() === 'ы')
					) {
						e.preventDefault();
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

					this.currentCategory.settings.forEach((setting) => {
						if (setting.key === 'task_name_pattern_date&time') {
							this.form.title = moment().format(setting.value);
						}
						if (setting.key === 'approximately_time') {
							this.form.approximately_time = parseInt(setting.value);
						}
					});
				}
			},
			getTaskSettingValue(key) {
				return this.form.settings?.find((item) => item.key === key)?.value;
			},
			async loadModel() {
				try {
					this.form = await getTask(this.taskId);

					this.$store.commit('currentOpenedTaskId', this.form.id);
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
							await this.saveTask();
						}
					}
				}
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
					await this.$store.dispatch('reloadTasks');

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
						await this.initComponent();
					}
				} catch (e) {
					if (e.response && e.response && e.response.data.errors) {
						this.errors = e.response.data.errors;
					}
				}
			},
			async saveTask(start = false) {
				try {
					this.isSaving = true;
					this.prepareForm();
					const data = await updateTask(this.taskId, this.form);
					this.$emit('updated');
					await this.$store.dispatch('reloadTasks');

					if (data.approximately_time) {
						this.approximatelyTime = this.toHHMM(data.approximately_time);
					}
					this.form = data;

					await this.saveSettings(this.settings);

					this.showAlert('Saved', 'The task was saved');
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
			secondsToStringTime(seconds) {
				const second = seconds % 60;
				let minute = ((seconds - second) / 60) | 0;
				const hour = (minute / 60) | 0;
				minute = minute - hour * 60;

				return `${this.prepareClockNumber(hour)}:${this.prepareClockNumber(
					minute,
				)}:${this.prepareClockNumber(second)}`;
			},
			stringTimeToSeconds(stringTime) {
				let times = stringTime.split(':');
				times = times.map(parseInt);
				return times[0] * 60 * 60 + times[1] * 60 + times[2];
			},
			prepareClockNumber(num) {
				return num < 10 ? '0' + num : num;
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
			deleteCheckpoint(checkpointIndex) {
				this.form.checkpoints.splice(checkpointIndex, 1);
				++this.checkpointUpdateKey;
			},
			changeCheckpointInputField(checkpointIndex) {
				const inputType = this.form.checkpoints[checkpointIndex].inputType;
				this.form.checkpoints[checkpointIndex].inputType =
					!inputType || inputType === 'text' ? 'textarea' : 'text';
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
				this.$store.getters.pusher
					.private(`App.User.${this.$store.getters.user.id}`)
					.on('task-countdown-stopped', ({ task }) => {
						const isCountdownStarted = !!this.form.start_time;
						if (!isCountdownStarted) {
							return;
						}

						this.setFormDataWithDelay(task).then(() => {
							this.showAlert('Countdown stopped');
						});
					})
					.on('task-countdown-started', ({ task }) => {
						const isCountdownStarted = !!this.form.start_time;
						if (!isCountdownStarted) {
							this.setFormDataWithDelay(task).then(() => {
								this.showAlert('Countdown started');
							});
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
	.checkpoint-delete {
		top: 9px;
		right: 10px;
		cursor: pointer;
		opacity: 0.5;
		&:hover {
			opacity: 1;
		}
	}

	.checkpoint-index {
		width: fit-content;
		top: 7px;
		left: 10px;
		cursor: pointer;
		color: #00c300;
		opacity: 0.5;
		&:hover {
			opacity: 1;
		}
	}

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
