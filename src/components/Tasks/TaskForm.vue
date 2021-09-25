<template>
	<teleport to="title">
		{{ form.title || h1.main }}&nbsp;
	</teleport>
	<div class="sm:flex items-between text-center">
		<div ref="editing_task_category" v-if="!isCreatingTask">
			<modal
				v-if="isShowModalCategory"
				:modal-width="500"
				:is-center="true"
				:close-on-bg-click="false"
				@close="isShowModalCategory = false">
				<template #modal-body>
					<form @submit.prevent="updateCategory">
						<div>
							<label :class="`block text-sm text-left font-bold bg-gray-400 mb-2 mt-2 text-black ${$color('taskSettingTextColor')}`" for="">
								Category
								<vue-select
									label="title"
									:options="categoriesSelectOptions"
									v-model="currentCategoryOptionInSelect"
								/>
							</label>
						</div>
						<div>
							<label :class="`block text-sm text-left font-bold bg-gray-400 mb-2 mt-2 text-black ${$color('taskSettingTextColor')}`" for="">
								Estimated time
								<input-field extra-class="bg-gray-400" v-model="form.approximately_time" :errors="errors.approximately_time" type="time_in_seconds" placeholder="Enter approximately time"/>
							</label>
						</div>

						<label for="settings" class="tc-block text-gray-700 text-sm font-bold mb-5">
							Settings
						</label>
						<div>
							<div v-for="(setting, index) in availableSettings" id="settings">
								<label :for="`setting-${setting.id}`" class="tc-block text-gray-700 text-sm font-bold mb-2">
									{{ setting.name }}
								</label>
								<div class="relative mb-4">
									<select :id="`setting-${setting.id}`"
													v-if="!setting.show_custom_value_input"
													class="tc-block appearance-none w-full bg-white border border-gray-300 px-4 py-2 pr-8 rounded shadow leading-tight focus:outline-none focus:shadow-outline"
													v-model="settings[index].value">
										<option value="" class="text-gray-500">Choose default value</option>
										<option v-for="(c, i) in setting.default_values" :key="i" :value="c.value">
											{{ c.value }}
										</option>
									</select>
									<div
										v-else-if="setting.custom_value_available"
									>
										<input-field
											:extra-class="`bg-gray-400  ${$color('taskSettingTextColor')}`"
											:id="`setting-${setting.id}`"
											:type="setting.component_type"
											:placeholder="setting.description"
											v-model="settings[index].value"
											:tag="settings[index].id = setting.id"
										/>
									</div>
									<small v-if="!setting.show_custom_value_input">{{ setting.description }}</small>
									<div class="b-switch-list" v-if="setting.custom_value_available">
										<div
											class="b-switch-list__item"
											v-if="setting.default_values && setting.default_values.length > 0"
										>
											<label class="b-switch">
												<input type="checkbox" name="show_tooltips" v-model="setting.show_custom_value_input" @change="settings[index].value = ''">
												<span></span>
											</label>
											<div
												class="b-switch-list__text"
											>
												<div class="b-switch-list__title" :class="$color('settingsTextColor')">Set custom value</div>
											</div>
										</div>
									</div>
								</div>
							</div>
						</div>

						<div class="flex flex-nowrap items-center mt-5">
							<button
								type="button"
								@click="isShowModalCategory = false"
								class="tc-block w-2/4 mr-1 bg-gray-700 text-white p-2 rounded">
								Cancel
							</button>
							<button
								type="submit"
								class="tc-block w-2/4 mr-1 bg-blue-700 text-white p-2 rounded">
								Update
							</button>
						</div>
					</form>
				</template>
			</modal>
		</div>
	</div>
	<div ref="modal" :class="`container mx-auto ${$color('blocks')} overflow-hidden ${isModal ? 'h-full' : ''} rounded-lg relative`">
		<div ref="header" :class="`p-3 shadow-md ${isModal ? '' : 'mt-10'}`">
			<router-link
				v-if="!isCreatingTask"
				:to="!currentCategory ? '/' : `/projects-categories/${currentCategory.id}/children/${getCategoryStatus()}`"
				class="rounded focus:outline-none sm:mb-0"
				:class="`${$color('h1')}-800`"
				type="button">
				{{ currentCategory ? currentCategory.title : 'Tasks' }}
			</router-link>
			<p v-else>
				Create task
			</p>
			<input-field
				v-if="!isCreatingTask && workspaceStatuses.length > 0"
				v-model="form.status_id"
				type="select"
				:hide-border="true"
				:options="workspaceStatuses"
				option-name-key="name"
				option-value-key="id"
				class="inline-block ml-3"
				style="min-width: 200px"
			/>
			<button
				type="button"
				:class="`checkpoint-delete absolute right-0`"
				v-if="isModal"
			>
				<span class="material-icons text-2xl text-red-700" @click="$emit('close')">close</span>
			</button>
		</div>

		<div :style="`height: ${middleBlockHeight}px`" class="text-center p-2.5">
			<div class="mt-14">
				<NewCountdown
					v-if="form.id"
					:init-task="form"
					@toggle="toggleCountdown"
					@update:seconds="updateSeconds"
				/>
				<NewCountdown
					v-else
					disabled
					:init-task="form"
					@update:seconds="updateSeconds"
				/>
			</div>
			<div class="mt-10 mb-5">
				<input-field
					v-model="form.title"
					:errors="errors.title"
					type="text"
					:hide-border="true"
					:extra-class="`mb-1 ${$color('input')}`"
					placeholder="Task name"/>
				<input-field
					v-model="form.description"
					:hide-border="true"
					:errors="errors.description"
					type="contenteditable"
					placeholder="Description"
				/>
			</div>
			<div v-if="!isCreatingTask" :class="`${$color('blocks')} rounded`" :key="checkpointUpdateKey">
				<div v-if="form.checkpoints.length">
					<div v-for="(checkpoint, v) in form.checkpoints" :key="v" class="flex mb-1">
						<div class="w-full relative">
								<span
									:class="`absolute left-0 top-0 mt-1.5 ml-1.5 z-10 ${checkpoint.inputType === 'textarea' ? 'pt-10' : ''}`"
								>{{ v + 1 }}</span>
							<input-field
								:type="checkpoint.inputType"
								placeholder="Checkpoint content"
								:hide-border="true"
								v-model="checkpoint.description"
								:extra-class="`pl-7 ${$color('input')} ${checkpoint.inputType === 'textarea' ? 'pt-10' : 'truncate pr-44'}`"
							/>
							<span class="absolute right-0 top-0 mt-1.5 text-sm" >
									{{ secondsToStringTime(checkpoint.start) }} - {{ secondsToStringTime(checkpoint.end) }}
									<span class="material-icons text-lg text-blue-700 checkpoint-delete" @click="changeCheckpointInputField(v)">edit</span>
									<span class="material-icons text-lg text-red-700 checkpoint-delete" @click="deleteCheckpoint(v)">delete</span>
								</span>
						</div>
						<!-- <p class="text-red-500 text-xs italic">Please type a category name</p> -->
					</div>
					<h3 class="text-sm">
						Add a checkpoint
						<span class="material-icons text-lg text-gray-500 checkpoint-delete" @click="addCheckpoint">add</span>
					</h3>
				</div>
				<div v-else>
					<h3 class="text-sm">
						Create checkpoints
						<span class="material-icons text-lg text-gray-500 checkpoint-delete" @click="addCheckpoint">add</span>
					</h3>
				</div>
			</div>
		</div>

		<div ref="footer" :class="`w-full p-5 shadow-top z-10 rounded-lg ${$color('blocks')} ${isModal ? 'absolute bottom-0' : ''}`">
			<task-actions
				:is-creating-task="isCreatingTask"
				:is-data-edited="isDataEdited"
				:is-saving="isSaving"
				@removeTask="removeTask(form)"
				@createTask="createTask"
				@saveTask="saveTask"
				@settingsTask="showModalCategory"
			/>
		</div>
		<p v-if="form.approximately_time" class="text-gray-500 pl-4 pb-2">
			Estimated time to complete the task: {{ toHHMM(form.approximately_time) }}
		</p>
	</div>

	<confirm
		v-if="confirm"
		:title="confirm.title"
		:body="confirm.body"
		@onOk="confirm.action()"
		@onCancel="confirm = undefined"
	/>
</template>

<script>
	import moment from 'moment'
	import InputField from "src/components/UIElements/InputField";
	import TaskActions from "src/components/UIElements/Tasks/TaskActions";
	import NewCountdown from "components/Tasks/NewCountdown";
	import Confirm from "components/UIElements/Confirm";

	export default {
		name: 'TaskForm',
		components: {
			Confirm,
			NewCountdown,
			TaskActions,
			InputField
		},
		props: {
			isModal: {
				required: false,
				type: Boolean,
				default: false
			},
			modalTaskId: {
				required: false,
				type: Number,
				default: null
			},
			statusId: {
				required: false,
				type: Number,
				default: null
			},
			modalProjectCategoryId: {
				required: false,
				type: Number,
				default: null
			}
		},
		emits: [
			'close',
			'updated'
		],
		data() {
			return {
				confirm: null,
				middleBlockHeight: 300,
				savedData: {},
				availableSettings: [],
				settings: [],
				isSaving: false,
				autoSaveTimeout: null,
				watchingFields: [
					'title',
					'description',
					'status_id',
					'project_category_id',
					'checkpoints',
					'approximately_time',
					'settings'
				],
				errors: {},
				showEditDescription: false,
				panel: false,
				isOpen: false,
				checkpointUpdateKey: 0,
				counter: {
					common_time: 0,
					start_time: 0
				},
				statuses: [
					{value: 'created', name: `Created`},
					{value: 'active', name: `Active`},
					{value: 'done', name: `Done`}
				],
				h1: {
					main: `${this.taskId ? 'Edit' : 'Create'} task`,
					CurrentTasksList: 'Current tasks',
					HiddenTasksList: 'Hidden tasks',
					ArchiveTasksList: 'Archive tasks'
				},
				selected: false,
				form: {
					title: '',
					status: 'created',
					status_id: this.statusId,
					project_category_id: this.projectCategoryId || '',
					description: '',
					common_time: 0,
					checkpoints: []
				},
				isShowModalCategory: false,
				categoriesSelectOptions: [],
				currentCategory: '',
				approximatelyTime: null,
				currentCategoryOptionInSelect: null,
				prevValue: null
			}
		},
		watch: {
			form (newVal) {
				this.setSavedData(newVal)
			}
		},
		computed: {
			taskId () {
				return this.projectCategoryId ? null : (this.form?.id || this.modalTaskId || this.$route.params.id)
			},
			workspaceStatuses () {
				return this.$store.getters.statuses
			},
			isCreatingTask() {
				return !this.taskId
			},
			projectCategoryId() {
				return this.form?.id ? null : (this.$route.params.project_category_id || this.modalProjectCategoryId)
			},
			isDataEdited () {
				if (!this.form.id) {
					return false
				}
				for(let i = 0; i < this.watchingFields.length; ++i) {
					const field = this.watchingFields[i]

					if (field === 'checkpoints') {
						const first = this.removeFieldsFromArray(this.savedData[field], ['end'])
						const second = this.removeFieldsFromArray(this.form[field], ['end'])
						if (!this.equals(first, second)) {
							if (this.equals(this.prevValue, second)) {
								return true
							}
							this.prevValue = JSON.parse(JSON.stringify(second))
							this.dispatchAutoSave()
							return true
						}
					} else if (!this.equals(this.savedData[field], this.form[field])) {
						this.dispatchAutoSave()
						return true
					}
				}
				return false
			}
		},
		methods: {
			showConfirm (title, body, action) {
				this.confirm = { title, body, action }
			},
			async removeTask (task) {
				const deleteTask = async () => {
					try {
						const {data: {data}} = await this.$axios.delete(`/tasks/${task.id}`)
						task.deleted_at = data.deleted_at
					} catch (e) {
						console.error(e)
					} finally {
						this.confirm = null
						this.$emit('close')
					}
				}
				this.showConfirm('Delete task', 'Are you sure?', deleteTask)
			},
			calcMiddleBlockHeight() {
				const modalHeight = this.$refs.modal.offsetHeight;
				const headerHeight = this.$refs.header.offsetHeight;
				const footerHeight = this.$refs.footer.offsetHeight;

				this.middleBlockHeight = modalHeight - (headerHeight + footerHeight);
				console.log('this.middleBlockHeight', this.middleBlockHeight);
			},
			async loadTaskSettings() {
				const {data: {data}} = await this.$axios.get('tasks/settings')
				this.initSettings(data, this.form.settings)
				this.availableSettings = data
			},
			initSettings(availableSettings, settings = []) {
				return availableSettings.map((item, index) => {
					const setting = this.getSettingById(settings, item.id, {
						id: item.id,
						value: ''
					})
					this.settings[index] = setting
					item.show_custom_value_input = item.default_values && item.default_values.findIndex(val => val.value === setting.value) === -1
				})
			},
			getSettingById(settings, id, defaultResult = null) {
				return settings.find(setting => setting.id === id) || defaultResult
			},
			async saveSettings(settings) {
				const {data: {data}} = await this.$axios.put(`/tasks/${this.form.id}/settings`, settings)
				this.initSettings(this.availableSettings, data.settings)
			},
			setFormDataWithDelay(data, delay = 200) {
				return new Promise((resolve, reject) => {
					if (this.form.id === data.id) {
						this.form.id = null
						setTimeout(() => {
							this.form = data
							resolve()
						}, delay)
					}
				})
			},
			dispatchAutoSave() {
				this.removeDispatchedAutoSave()
				this.autoSaveTimeout = setTimeout(this.saveTask, 5000)
			},
			removeDispatchedAutoSave() {
				if (this.autoSaveTimeout) {
					clearTimeout(this.autoSaveTimeout)
				}
			},
			equals (o1, o2) {
				return JSON.stringify(o1) === JSON.stringify(o2)
			},
			removeFieldsFromArray (arr, fieldsForDelete) {
				if (!arr) {
					return
				}
				arr = JSON.parse(JSON.stringify(arr))
				return arr.map(item => {
					const keys = Object.keys(item).filter(key => !fieldsForDelete.includes(key))
					const result = {}
					keys.forEach(key => result[key] = item[key])
					return result
				})
			},
			async showModalCategory () {
				try {
					this.isShowModalCategory = true
					if (this.categoriesSelectOptions.length === 0) {
						await this.loadCategories()
					}
				} catch (e) {
					console.error(e)
				}
			},
			async updateCategory () {
				if (this.currentCategoryOptionInSelect) {
					this.form.project_category_id = this.currentCategoryOptionInSelect.id
				}
				this.isShowModalCategory = false
				await this.saveTask()
				await this.loadCategory()
			},
			getShortcutSaveListener() {
				return (e) => {
					if (e.ctrlKey && (e.key.toLowerCase() === 's' || e.key.toLowerCase() === 'ы')) {
						e.preventDefault();
						this.saveTask()
					}
				}
			},
			getActions() {
				const actions = [
					{
						click: () => {
							this[this.form.id ? 'save' : 'create']()
						},
						label: this.form.id ? 'Save' : 'Create'
					}
				]
				actions.push({
					click: () => {
						this[this.isCreatingTask ? 'cancel' : 'goToCurrentTasks']()
					},
					label: this.isCreatingTask ? 'Cancel' : 'Tasks'
				})
				return actions
			},
			async loadCategories() {
				const {data: {data}} = await this.$axios.get('project_categories?all')
				this.categoriesSelectOptions = data
			},
			async loadCategory() {
				if (this.projectCategoryId || this.form.project_category_id) {
					const {data: {data}} = await this.$axios.get(`project_categories/${this.projectCategoryId || this.form.project_category_id}`)
					this.currentCategory = data
					this.currentCategoryOptionInSelect = data

					if (!!this.form.id || this.currentCategory.settings.length === 0) {
						return
					}

					this.currentCategory.settings.forEach(setting => {
						if (setting.key === 'task_name_pattern_date&time') {
							this.form.title = moment().format(setting.value)
						}
						if (setting.key === 'approximately_time') {
							this.form.approximately_time = parseInt(setting.value)
						}
					})
				}
			},
			async loadModel() {
				const {data: {data}} = await this.$axios.get(`tasks/${this.taskId}`)
				data.common_time = data.common_time || 0
				this.form = data

				this.$store.commit('currentOpenedTaskId', this.form.id)
			},
			setSavedData(data) {
				this.watchingFields.forEach(field => this.savedData[field] = JSON.parse(JSON.stringify(data[field])))
			},
			toHHMM (seconds) {
				let hours   = Math.floor(seconds / 3600);
				let minutes = Math.floor((seconds - (hours * 3600)) / 60);

				if (hours   < 10) {
					hours   = "0" + hours;
				}
				if (minutes < 10) {
					minutes = "0" + minutes;
				}
				return hours + ':' + minutes;
			},
			getCategoryStatus() {
				if (this.form.status === 'created' || this.form.status === 'active') {
					return 'current'
				}
				return this.form.status
			},
			async toggleCountdown() {
				const id = this.taskId
				this.form.id = null
				const { data: {data} } = await this.$axios[this.form.start_time ? 'delete' : 'post'](`tasks/${id}/countdown`)
				this.form = { ...data }
				this.form.id = id
				this.updateSeconds(this.form.common_time)

				if (this.form.start_time && this.form.status_id) {
					const statusCurrent = this.workspaceStatuses.find((el) => el.type !== 'active');
					if (statusCurrent) {
						const firstActiveStatus = this.workspaceStatuses.find((el) => el.type === 'active');
						if (firstActiveStatus) {
							this.form.status_id = firstActiveStatus.id;
							this.saveTask();
						}
					}
				}
			},
			prepareForm() {
				if (this.form.project_category_id === '') {
					delete this.form.project_category_id
				}
			},
			async createTask () {
				try {
					this.prepareForm()
					const {data: {data}} = await this.$axios.post('tasks', this.form)
					this.$emit('updated')
					this.$store.dispatch('reloadTasks')
					if (!this.isCreatingTask) {
						this.showAlert()
					}
					if (!this.isModal) {
						await this.$router.push({
							name: 'TasksEdit',
							params: {
								id: data.id
							}
						})
					} else {
						this.form = data
						this.created()
					}
				} catch (e) {
					if (e.response && e.response && e.response.data.errors) {
						this.errors = e.response.data.errors
					}
				}
			},
			async saveTask () {
				try {
					this.isSaving = true
					this.prepareForm()
					const {data: {data}} = await this.$axios.put(`tasks/${this.taskId}`, this.form)
					this.$emit('updated')
					this.$store.dispatch('reloadTasks')
					if (data.approximately_time) {
						this.approximatelyTime = this.toHHMM(data.approximately_time)
					}
					this.form = data

					await this.saveSettings(this.settings)

					this.showAlert('Saved', 'The task was saved')
					this.removeDispatchedAutoSave()
					const id = this.form.id
					this.form.id = null
					setTimeout(() => this.form.id = id, 1)
				} catch (e) {
					if (e.response && e.response && e.response.data.errors) {
						this.errors = e.response.data.errors
					}
				} finally {
					this.isSaving = false
				}

			},
			goToCurrentTasks() {
				this.$router.push('/')
			},
			secondsToStringTime(seconds) {
				const second = seconds % 60
				let minute = (seconds - second) / 60 | 0
				const hour = minute / 60 | 0
				minute = minute - (hour * 60)

				return `${this.prepareClockNumber(hour)}:${this.prepareClockNumber(minute)}:${this.prepareClockNumber(second)}`
			},
			stringTimeToSeconds(stringTime) {
				let times = stringTime.split(':')
				times = times.map(parseInt)
				return times[0] * 60 * 60 + times[1] * 60 + times[2]
			},
			prepareClockNumber(num) {
				return num < 10 ? '0' + num : num
			},
			addCheckpoint() {
				const { form } = this
				if (!form.checkpoints) {
					form.checkpoints = []
				}
				const timeSinceStartTime = !form.start_time ? 0 : Math.floor(
					((new Date()) - (new Date()).setTime(form.start_time * 1000)) / 1000
				)
				const currentTime = form.common_time + timeSinceStartTime
				if (form.checkpoints.length > 0) {
					const prevCheckpointIndex = form.checkpoints.length - 1
					form.checkpoints[prevCheckpointIndex].end = currentTime
				}
				form.checkpoints.push({
					description: 'New one',
					start: currentTime,
					end: currentTime
				})
				++this.checkpointUpdateKey
			},
			deleteCheckpoint(checkpointIndex) {
				this.form.checkpoints.splice(checkpointIndex, 1)
				++this.checkpointUpdateKey
			},
			changeCheckpointInputField(checkpointIndex) {
				const inputType = this.form.checkpoints[checkpointIndex].inputType;
				this.form.checkpoints[checkpointIndex].inputType = !inputType || inputType === 'text' ? 'textarea' : 'text'
				console.log(this.form.checkpoints[checkpointIndex].inputType);
				++this.checkpointUpdateKey
			},
			updateSeconds(seconds) {
				if (!this.form.checkpoints || this.form.checkpoints.length === 0) {
					return
				}
				this.form.checkpoints[this.form.checkpoints.length - 1].end = seconds
			},
			onResize() {
				this.calcMiddleBlockHeight()
			}
		},
		mounted() {
			this.calcMiddleBlockHeight()
			console.log('TASK FORM',this.statusId)
		},
		beforeUnmount() {
			window.removeEventListener("resize", this.onResize);
		},
		async created () {
			window.addEventListener("resize", this.onResize);
			if (this.taskId) {
				await this.loadModel()
				window.onkeydown = this.getShortcutSaveListener()
			}

			if (this.projectCategoryId && this.isCreatingTask) {
				this.form.project_category_id = this.projectCategoryId
			}
			await this.loadCategory()
			await this.loadTaskSettings()
			this.$store.getters.pusher.private(`App.User.${this.$store.getters.user.id}`)
				.on('task-countdown-stopped', ({task}) => {
					const isCountdownStarted = !!this.form.start_time
					if (!isCountdownStarted) {
						return
					}

					this.setFormDataWithDelay(task).then(() => {
						this.showAlert('Countdown stopped')
					})
				})
				.on('task-countdown-started', ({task}) => {
					const isCountdownStarted = !!this.form.start_time
					if (!isCountdownStarted) {
						this.setFormDataWithDelay(task).then(() => {
							this.showAlert('Countdown started')
						})
					}
				})
		},
	}
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
		--tw-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1), 0 2px 4px -1px rgba(0, 0, 0, 0.06);
		box-shadow: 0 -4px 6px -1px rgba(0, 0, 0, 0.16);
	}
</style>
