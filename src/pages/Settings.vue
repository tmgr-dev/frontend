<template>
	<teleport to="title"> Settings </teleport>

	<BaseLayout>
		<template #header> Settings </template>

		<template #body>
			<div class="w-full md:flex">
				<div
					class="flex justify-between p-4 md:w-1/4 md:flex-col md:justify-start md:gap-4"
				>
					<p
						class="cursor-pointer font-bold transition-colors duration-300 hover:underline hover:decoration-2 md:text-lg"
						@click="showNotificationSettings"
						:class="[
							isNotification
								? 'text-blue-800 underline decoration-2 dark:text-amber-400 '
								: '',
						]"
					>
						Notification
					</p>
					<div
						class="cursor-pointer font-bold transition-colors duration-300 hover:underline hover:decoration-2 md:text-lg"
						@click="showWorkspaceSettings"
						:class="[
							isWorkspaceSettings
								? 'text-blue-800 underline decoration-2 dark:text-amber-400 '
								: '',
						]"
					>
						Workspace settings
					</div>
					<div
						class="cursor-pointer font-bold transition-colors duration-300 hover:underline hover:decoration-2 md:text-lg"
						@click="showProfileSettings"
						:class="[
							isProfile
								? 'text-blue-800 underline decoration-2 dark:text-amber-400 '
								: '',
						]"
					>
						Profile
					</div>
				</div>
				<div class="md:w-3/4">
					<div v-if="isNotification" class="flex flex-col gap-3.5 p-4 md:w-1/2">
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

					<div
						v-if="isWorkspaceSettings"
						class="flex flex-col gap-3 p-4 md:w-1/2"
					>
						<div>
							<div v-for="(setting, index) in availableSettings">
								<label
									:for="`setting-${setting.id}`"
									class="mb-2 block text-sm font-bold text-gray-700"
								>
									{{ setting.name }}
								</label>

								<div class="relative mb-4">
									<template
										v-if="setting.component_type === 'current_workspace'"
									>
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
					<div v-if="isProfile" class="flex flex-col gap-3.5 p-2">
						<profile />
						<div class="mt-6 border-t pt-6">
							<h3 class="mb-4 text-lg font-bold">Telegram Integration</h3>
							<div class="flex items-center gap-4">
								<div v-if="user.telegram_username" class="flex items-center gap-2">
									<span class="text-green-600">✓</span>
									<span>Connected as @{{ user.telegram_username }}</span>
									<button
										class="ml-2 text-red-500 hover:text-red-700"
										@click="unlinkTelegram"
									>
										Unlink
									</button>
								</div>
								<button
									v-else
									class="rounded bg-blue-500 py-2 px-4 font-bold text-white transition hover:bg-blue-600 focus:outline-none"
									@click="generateTelegramLink"
								>
									Connect Telegram
								</button>
							</div>
						</div>
					</div>
				</div>
			</div>

			<!-- Add to your existing confirm modal -->
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

						<a v-if="confirm.link"
						class="mt-2 block text-blue-500 hover:text-blue-700"
						:href="confirm.link"
						target="_blank"
						>
						Open Telegram
						</a>
					</template>
				</confirm>
			</Transition>
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
	import Profile from 'src/pages/Profile.vue';
	import { generateLink, unlink } from 'src/actions/tmgr/telegram';

	export default {
		name: 'Settings',
		components: {
			Profile,
			TextField,
			TimeField,
			Switcher,
			Select,
			Button,
			Confirm,
			CurrentWorkspace,
		},
		created() {
			this.handleTabFromQuery();
		},
		data: () => ({
			availableSettings: [],
			settings: [],
			user: {},
			confirm: null,
			isNotification: false,
			isWorkspaceSettings: true,
			isProfile: false,
			telegramLink: null,
		}),
		watch: {
			'$route.query': {
				handler: 'handleTabFromQuery',
				immediate: true
			}
		},
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
			handleTabFromQuery() {
				const tab = this.$route.query.tab;
				if (tab) {
					switch (tab.toLowerCase()) {
						case 'notification':
							this.showNotificationSettings();
							break;
						case 'workspace':
							this.showWorkspaceSettings();
							break;
						case 'profile':
							this.showProfileSettings();
							break;
					}
				}
			},

			showNotificationSettings() {
				this.isWorkspaceSettings = false;
				this.isProfile = false;
				this.isNotification = true;
				this.updateQueryParam('notification');
			},

			showWorkspaceSettings() {
				this.isNotification = false;
				this.isWorkspaceSettings = true;
				this.isProfile = false;
				this.updateQueryParam('workspace');
			},

			showProfileSettings() {
				this.isNotification = false;
				this.isWorkspaceSettings = false;
				this.isProfile = true;
				this.updateQueryParam('profile');
			},

			// Add new method to update query parameter
			updateQueryParam(tab) {
				// Update URL without reloading the page
				this.$router.push({
					query: { ...this.$route.query, tab }
				}).catch(() => {});
			},
			async generateTelegramLink() {
				try {
					const link = await generateLink();

					this.showConfirm(
						'Connect Telegram',
						'Click the button below to open Telegram and connect your account. After connecting, you\'ll receive notifications through Telegram.',
						() => {
							window.open(link, '_blank');
							this.confirm = null;
						},
						link
					);
				} catch (error) {
					console.error('Failed to generate Telegram link:', error);
				}
			},

			async unlinkTelegram() {
				this.showConfirm(
					'Unlink Telegram',
					'Are you sure you want to unlink your Telegram account? You will no longer receive notifications through Telegram.',
					async () => {
						try {
							await unlink();
							this.user = await getUser();
							this.confirm = null;
						} catch (error) {
							console.error('Failed to unlink Telegram:', error);
						}
					}
				);
			},

			showConfirm(title, body, action, link = null) {
				this.confirm = { title, body, action, link };
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
