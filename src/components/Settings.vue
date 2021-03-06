<template>
	<teleport to="title">
		Settings
	</teleport>
	<BaseLayout>
		<template #header>
			Settings
		</template>
		<template #body>
			<div :class="`w-full md:w-auto md:flex-grow md:flex md:items-center md:block block ${$color('textMain')}`">
				<input-field v-model="userSettings.showTooltips" type="checkbox" placeholder="Show tooltips"></input-field>
				<div>
					<div v-for="(setting, index) in availableSettings">
						<label :for="`setting-${setting.id}`" class="tc-block text-gray-700 text-sm font-bold mb-2">
							{{ setting.name }}
						</label>
						<div class="relative mb-4">
							<input-field
								v-if="!setting.show_custom_value_input"
								:id="`setting-${setting.id}`"
								type="select"
								:placeholder="setting.description"
								v-model="settings[index].value"
								:options="setting.default_values"
								option-name-key="value"
								option-value-key="value"
								:tag="settings[index].id = setting.id"
							/>
							<div
								v-else-if="setting.custom_value_available"
							>
								<input-field
									:id="`setting-${setting.id}`"
									:type="setting.component_type"
									:placeholder="setting.description"
									v-model="settings[index].value"
									:tag="settings[index].id = setting.id"
								/>
							</div>
							<small v-if="!setting.show_custom_value_input">{{ setting.description }}</small>
							<div class="b-switch-list mt-3" v-if="setting.custom_value_available">
								<div
									class="b-switch-list__item"
									v-if="setting.default_values && setting.default_values.length > 0"
								>
									<label class="b-switch">
										<input type="checkbox" name="show_tooltips" v-model="setting.show_custom_value_input" @change="settings[index].value = ''">
										<span></span>
									</label>
									<div class="b-switch-list__text">
										<div class="b-switch-list__title" :class="$color('settingsTextColor')">Set custom value</div>
									</div>
								</div>
							</div>
						</div>
					</div>
				</div>
				<div class="text-right">
					<button
						@click="updateSettings"
						class="bg-blue-500 mr-5 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded focus:outline-none sm:mb-0 mt-10"
						type="button">
						Save
					</button>
				</div>
			</div>
			<alert v-if="successSavingShow" />
		</template>
	</BaseLayout>
</template>

<script>
	export default {
		name: 'Settings',
		data: () => ({
			successSavingShow: false,
			availableSettings: [],
			settings: [],
			user: {},
		}),
		computed: {
			userSettings () {
				return this.$store.getters.getUserSettings || {}
			}
		},
		async mounted() {
			await this.loadUser()
			await this.loadSettings()
		},
		methods: {
			async loadUser() {
				const {data: {data}} = await this.$axios.get('user')
				this.user = data
			},
			async loadSettings() {
				const {data: {data}} = await this.$axios.get('v2/user/settings')
				this.initSettings(data, this.user.settings)
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
				const result = settings.find(setting => setting.id === id) || defaultResult
				return result
			},
			async saveSettings(settings) {
				const {data: {data}} = await this.$axios.put(`/v2/user/settings`, settings)
				this.initSettings(this.availableSettings, data.settings)
			},
			async updateSettings () {
				try {
					await this.$axios.put('user/settings', {
						settings: this.userSettings
					})
					await this.saveSettings(this.settings)
					this.showAlert()
				} catch (e) {
					console.error(e)
				}
			},
			showAlert () {
				this.successSavingShow = true
				setTimeout(() => this.successSavingShow = false, 2000)
			}
		}
	}
</script>

<style scoped>
	.settings-container {
		max-width: 700px;
		margin: 50px auto;
		padding: 20px;
		box-shadow: rgb(233 233 233) 1px 4px 20px;
	}
</style>
