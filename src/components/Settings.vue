<template>
	<teleport to="title"> Settings </teleport>
	<BaseLayout>
		<template #header> Settings </template>
		<template #body>
			<div class="flex flex-col gap-3 max-w-md">
				<button
					v-if="!pusherBeamsUserId"
					class="px-5 py-2 border-green-400 text-green-600 hover:bg-green-400 text-green-400 hover:text-white border-2 transition"
					@click="togglePushes"
				>
					Web Pushes
				</button>

				<button
					class="px-5 py-2 border-blue-400 text-blue-400 hover:bg-blue-400 hover:text-white border-2 transition"
					@click="testWebPushNotifications"
				>
					Test web push notifications
				</button>

				<input-field
					v-model="userSettings.showTooltips"
					placeholder="Show tooltips"
					type="checkbox"
				/>

				<div>
					<div v-for="(setting, index) in availableSettings">
						<label
							:for="`setting-${setting.id}`"
							class="block text-gray-700 text-sm font-bold mb-2"
						>
							{{ setting.name }}
						</label>

						<div class="relative mb-4">
							<div v-if="setting.component_type === 'current_workspace'">
								<current-workspace v-model="settings[index].value" />
							</div>

							<input-field
								v-else-if="!setting.show_custom_value_input"
								:id="`setting-${setting.id}`"
								v-model="settings[index].value"
								:options="setting.default_values"
								:placeholder="setting.description"
								:tag="(settings[index].id = setting.id)"
								option-name-key="value"
								option-value-key="value"
								type="select"
							/>

							<div v-else-if="setting.custom_value_available">
								<input-field
									:id="`setting-${setting.id}`"
									v-model="settings[index].value"
									:placeholder="setting.description"
									:tag="(settings[index].id = setting.id)"
									:type="setting.component_type"
								/>
							</div>

							<small v-if="!setting.show_custom_value_input">
								{{ setting.description }}
							</small>

							<div
								v-if="setting.custom_value_available"
								class="b-switch-list mt-3"
							>
								<div
									v-if="
										setting.default_values && setting.default_values.length > 0
									"
									class="b-switch-list__item"
								>
									<label class="b-switch">
										<input
											v-model="setting.show_custom_value_input"
											name="show_tooltips"
											type="checkbox"
											@change="settings[index].value = ''"
										/>
										<span></span>
									</label>

									<div class="b-switch-list__text">
										<div
											class="b-switch-list__title text-gray-800 dark:text-gray-400"
										>
											Set custom value
										</div>
									</div>
								</div>
							</div>
						</div>
					</div>
				</div>

				<div class="text-left">
					<button
						class="bg-blue-500 hover:bg-blue-600 transition text-white font-bold py-2 px-8 rounded focus:outline-none sm:mb-0 mt-4"
						type="button"
						@click="updateSettings"
					>
						Save
					</button>
				</div>
			</div>

			<alert ref="alert" />
		</template>
	</BaseLayout>

	<Transition name="fade">
		<confirm
			v-if="confirm"
			:body="confirm.body"
			:title="confirm.title"
			@onCancel="confirm = undefined"
			@onOk="confirm.action()"
		>
			<template #body-content>
				<p>{{ confirm.body }}</p>
				<a
					class="text-blue"
					href="/push-notifications-enable-guide"
					target="_blank"
				>
					How to enable or disable Push Notifications on Chrome, Firefox &
					Safari Browser?
				</a>
			</template>
		</confirm>
	</Transition>
</template>

<script>
	import Confirm from 'src/components/UIElements/Confirm';
	import Button from 'src/components/UIElements/Button';
	import CurrentWorkspace from 'src/components/UIElements/CurrentWorkspace';

	export default {
		name: 'Settings',
		components: {
			Button,
			Confirm,
			CurrentWorkspace,
		},
		data: () => ({
			availableSettings: [],
			settings: [],
			user: {},
			confirm: null,
		}),
		computed: {
			userSettings() {
				return this.$store.getters.getUserSettings || {};
			},
			pusherBeamsUserId() {
				return this.$store.getters.pusherBeamsUserId;
			},
		},
		async mounted() {
			await this.loadUser();
			await this.loadSettings();
		},
		methods: {
			parseInt(v) {
				return parseInt(v);
			},
			showConfirm(title, body, action) {
				this.confirm = { title, body, action };
			},
			async testWebPushNotifications() {
				await this.$axios.post('test/web/notifications');
			},
			async togglePushes() {
				if (this.$store.getters.pusherBeamsUserId) {
					await this.$store.getters.pusherBeamsClient.stop();
					return this.$store.commit('pusherBeamsUserId', null);
				}
				const userId = this.$store.getters.user.id.toString();
				try {
					await this.$store.getters.pusherBeamsClient.start();
					await this.$store.getters.pusherBeamsClient.setUserId(
						userId,
						this.$store.getters.pusherTokenProvider,
					);
					this.$store.commit('pusherBeamsUserId', userId);
				} catch (e) {
					this.showConfirm(
						'Notifications registration',
						'Please check notifications permissions',
						() => {
							this.confirm = null;
						},
					);
				}
			},
			async loadUser() {
				const {
					data: { data },
				} = await this.$axios.get('user');
				this.user = data;
			},
			async loadSettings() {
				const {
					data: { data },
				} = await this.$axios.get('v2/user/settings');
				this.initSettings(data, this.user.settings);
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
				const {
					data: { data },
				} = await this.$axios.put(`/v2/user/settings`, settings);
				this.initSettings(this.availableSettings, data.settings);
			},
			async updateSettings() {
				try {
					await this.$axios.put('user/settings', {
						settings: this.userSettings,
					});
					await this.saveSettings(this.settings);
					this.showAlert();
				} catch (e) {
					console.error(e);
				}
			},
		},
	};
</script>

<style scoped>
	.settings-container {
		max-width: 700px;
		margin: 50px auto;
		padding: 20px;
		box-shadow: rgb(233 233 233) 1px 4px 20px;
	}
</style>
