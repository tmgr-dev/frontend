<template>
	<teleport to="title"> Settings </teleport>

	<BaseLayout>
		<template #header> Settings </template>

		<template #body>
			<div class="flex max-w-md flex-col gap-3">
				<button
					v-if="!pusherBeamsUserId"
					class="border-2 border-green-400 px-5 py-2 text-green-600 text-green-400 transition hover:bg-green-400 hover:text-white"
					@click="togglePushes"
				>
					Web Pushes
				</button>

				<button
					class="border-2 border-blue-400 px-5 py-2 text-blue-400 transition hover:bg-blue-400 hover:text-white"
					@click="testWebPushNotifications"
				>
					Test web push notifications
				</button>

				<Switcher
					name="show_tooltips"
					v-model="userSettings.showTooltips"
					placeholder="Show tooltips"
				/>

				<div>
					<div v-for="(setting, index) in availableSettings">
						<label
							:for="`setting-${setting.id}`"
							class="mb-2 block text-sm font-bold text-gray-700"
						>
							{{ setting.name }}
						</label>

						<div class="relative mb-4">
							<template v-if="setting.component_type === 'current_workspace'">
								<current-workspace
									v-model="settings[index].value"
									@updateSettings="updateSettings"
								/>
							</template>

							<template v-else-if="setting.custom_value_available">
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
							</template>

							<small v-if="!setting.show_custom_value_input">
								{{ setting.description }}
							</small>

							<Switcher
								v-if="
									setting.custom_value_available &&
									setting.default_values &&
									setting.default_values.length > 0
								"
								name="set_custom_value"
								v-model="setting.show_custom_value_input"
								placeholder="Set custom value"
							/>
						</div>
					</div>
				</div>

				<div class="text-left">
					<button
						class="mt-4 rounded bg-blue-500 py-2 px-8 font-bold text-white transition hover:bg-blue-600 focus:outline-none sm:mb-0"
						type="button"
						@click="updateSettings"
					>
						Save
					</button>
				</div>
			</div>
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
	import Confirm from 'src/components/general/Confirm.vue';
	import Button from 'src/components/general/Button.vue';
	import CurrentWorkspace from 'src/components/CurrentWorkspace.vue';
	import {
		getUser,
		getUserSettingsV2,
		updateUserSettings,
		updateUserSettingsV2,
	} from 'src/actions/tmgr/user';
	import { sendNotification } from 'src/actions/tmgr/notifications';
	import Select from 'src/components/general/Select.vue';
	import Switcher from 'src/components/general/Switcher.vue';
	import TimeField from 'src/components/general/TimeField.vue';
	import TextField from 'src/components/general/TextField.vue';

	export default {
		name: 'Settings',
		components: {
			TextField,
			TimeField,
			Switcher,
			Select,
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
				return this.$store.state.userSettings || {};
			},
			pusherBeamsUserId() {
				return this.$store.getters.getPusherBeamsUserId;
			},
		},
		async mounted() {
			this.user = await getUser();
			await this.loadSettings();
		},
		methods: {
			showConfirm(title, body, action) {
				this.confirm = { title, body, action };
			},
			async testWebPushNotifications() {
				await sendNotification();
			},
			async togglePushes() {
				if (this.$store.getters.getPusherBeamsUserId) {
					await this.$store.getters.getPusherBeamsClient.stop();
					return this.$store.commit('setPusherBeamsUserId', null);
				}
				const userId = this.$store.state.user?.id.toString();

				try {
					await this.$store.getters.getPusherBeamsClient.start();
					await this.$store.getters.getPusherBeamsClient.setUserId(
						userId,
						this.$store.getters.getPusherTokenProvider,
					);
					this.$store.commit('setPusherBeamsUserId', userId);
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
			async loadSettings() {
				const data = await getUserSettingsV2();
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
			async updateSettings() {
				try {
					const [data] = await Promise.all([
						updateUserSettingsV2(this.settings),
						updateUserSettings({ settings: this.userSettings }),
					]);
					this.initSettings(this.availableSettings, data.settings);
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
