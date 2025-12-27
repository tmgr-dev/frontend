<template>
	<teleport to="title"> Settings </teleport>

	<BaseLayout>
		<template #body>
			<div class="w-full md:flex">
				<div class="md:w-full">
					<div v-if="isNotification" class="flex flex-col gap-3.5 p-4">
						<h3 class="mb-4 text-lg font-bold">Notifications</h3>
						
						<NotificationSettingsForm />

						<div class="mt-8 border-t pt-6">
							<h4 class="mb-4 text-md font-semibold">Push notifications (legacy)</h4>
							<div class="flex flex-col gap-3 md:w-1/2">
								<button
									v-if="!pusherBeamsUserId"
									class="border-2 border-green-400 px-5 py-2 text-green-400 text-green-600 transition hover:bg-green-400 hover:text-white"
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
										class="mt-4 rounded bg-blue-500 px-8 py-2 font-bold text-white transition hover:bg-blue-600 focus:outline-none sm:mb-0"
										type="button"
										@click="updateSettings"
									>
										Save
									</button>
								</div>
							</div>
						</div>
					</div>

					<div
						v-if="isWorkspaceSettings"
						class="flex flex-col gap-3 p-4 md:w-1/2"
					>
						<h3 class="mb-4 text-lg font-bold">Worksapce Settings</h3>
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
									<template v-else-if="setting.component_type === 'select'">
										<Select
											v-model="settings[index].value"
											:options="
												setting.default_values.map((val) => ({
													label: val.value,
													value: val.value,
												}))
											"
											:placeholder="setting.description"
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
								class="mt-4 rounded bg-blue-500 px-8 py-2 font-bold text-white transition hover:bg-blue-600 focus:outline-none sm:mb-0"
								type="button"
								@click="updateSettings"
							>
								Save
							</button>
						</div>
					</div>

					<div v-if="isProfile" class="flex flex-col gap-3.5 p-2">
						<h3 class="mb-4 text-lg font-bold">Profile</h3>
						<profile />
						<div class="mt-6 border-t pt-6">
							<h3 class="mb-4 text-lg font-bold">Telegram Integration</h3>
							<div class="flex items-center gap-4">
								<div
									v-if="user.telegram_username"
									class="flex items-center gap-2"
								>
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
									class="rounded bg-blue-500 px-4 py-2 font-bold text-white transition hover:bg-blue-600 focus:outline-none"
									@click="generateTelegramLink"
								>
									Connect Telegram
								</button>
							</div>
						</div>
					</div>

					<div v-if="isDevice" class="flex flex-col gap-3.5 p-4">
						<div class="">
							<h3 class="mb-4 text-lg font-bold">Smart Device Integration</h3>

							<!-- Token Display Section -->
							<div class="mb-6">
								<div class="flex flex-col space-y-2">
									<label class="text-sm font-medium text-gray-700"
										>API Token</label
									>
									<div class="flex items-center gap-2">
										<TextField
											:type="showToken ? 'text' : 'password'"
											:model-value="user.smart_device_token"
											class="flex-1"
											placeholder="Token needs to be generated"
											readonly="true"
										/>
										<button
											class="ml-2 px-3 py-1 text-sm"
											@click="showToken = !showToken"
											:class="[
												showToken
													? 'text-gray-600 hover:text-gray-800'
													: 'text-blue-600 hover:text-blue-800',
											]"
										>
											{{ showToken ? 'Hide' : 'Show' }}
										</button>
									</div>
								</div>
							</div>

							<!-- Token Management Buttons -->
							<div class="flex flex-wrap gap-3">
								<button
									class="rounded bg-blue-500 px-4 py-2 font-semibold text-white transition hover:bg-blue-600 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2"
									@click="generateSmartDeviceToken"
								>
									{{
										user.smart_device_token
											? 'Generate New Token'
											: 'Generate Token'
									}}
								</button>
								<button
									v-if="user.smart_device_token"
									class="rounded bg-red-500 px-4 py-2 font-semibold text-white transition hover:bg-red-600 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2"
									@click="revokeSmartDeviceToken"
								>
									Revoke Token
								</button>
							</div>

							<!-- Token Information -->
							<div class="mt-6">
								<h4 class="mb-2 text-sm font-semibold text-gray-700">
									Token Information
								</h4>
								<div
									v-if="user.smart_device_token"
									class="text-sm text-gray-600"
								>
									<p>
										Created:
										{{ formatDate(user.smart_device_token_created_at) }}
									</p>
								</div>
							</div>

							<!-- Usage Instructions -->
							<div class="mt-6 rounded-md bg-gray-50 p-4">
								<h4 class="mb-2 text-sm font-semibold text-gray-700">
									How to Use
								</h4>
								<div class="space-y-2 text-sm text-gray-600">
									<p>1. Generate a token using the button above</p>
									<p>
										2. Include the token in your device's API requests using the
										header:
									</p>
									<code
										class="mt-1 block rounded bg-gray-100 p-2 font-mono text-sm"
									>
										X-Smart-Device-Token: your_token_here
									</code>
									<p class="mt-2 text-sm text-gray-500">
										Keep your token secure. If compromised, generate a new one
										immediately.
									</p>
								</div>
							</div>
						</div>
					</div>
				</div>
			</div>

			<!-- Confirmation Modal -->
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
							v-if="confirm.link"
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
</template>

<script>
	import Confirm from '@/components/general/Confirm.vue';
	import Button from '@/components/general/Button.vue';
	import CurrentWorkspace from '@/components/CurrentWorkspace.vue';
	import {
		getUser,
		getUserSettingsV2,
		updateUserSettings,
		updateUserSettingsV2,
	} from '@/actions/tmgr/user';
	import { sendNotification } from '@/actions/tmgr/notifications';
	import Select from '@/components/general/Select.vue';
	import Switcher from '@/components/general/Switcher.vue';
	import TimeField from '@/components/general/TimeField.vue';
	import TextField from '@/components/general/TextField.vue';
	import Profile from '@/pages/Profile.vue';
	import NotificationSettingsForm from '@/components/notifications/NotificationSettingsForm.vue';
	import { generateLink, unlink } from '@/actions/tmgr/telegram';
	import {
		generateSmartDeviceToken,
		revokeSmartDeviceToken,
	} from '@/actions/tmgr/smart-devices';
	import {
		BreadcrumbItem,
		BreadcrumbLink,
		BreadcrumbSeparator,
	} from '@/components/ui/breadcrumb';
	import store from '@/store/index.js';

	export default {
		name: 'Settings',
		components: {
			BreadcrumbSeparator,
			BreadcrumbItem,
			BreadcrumbLink,
			Profile,
			TextField,
			TimeField,
			Switcher,
			Select,
			Button,
			Confirm,
			CurrentWorkspace,
			NotificationSettingsForm,
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
			isDevice: false,
			telegramLink: null,
			showToken: false,
		}),
		watch: {
			'$route.query': {
				handler: 'handleTabFromQuery',
				immediate: true,
			},
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
			formatDate(date) {
				if (!date) return '';
				return new Date(date).toLocaleString();
			},

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
						case 'device':
							this.showDeviceSettings();
							break;
					}
				}
			},

			showNotificationSettings() {
				this.isWorkspaceSettings = false;
				this.isProfile = false;
				this.isNotification = true;
				this.isDevice = false;
				this.updateQueryParam('notification');
			},

			showWorkspaceSettings() {
				this.isNotification = false;
				this.isWorkspaceSettings = true;
				this.isProfile = false;
				this.isDevice = false;
				this.updateQueryParam('workspace');
			},

			showProfileSettings() {
				this.isNotification = false;
				this.isWorkspaceSettings = false;
				this.isProfile = true;
				this.isDevice = false;
				this.updateQueryParam('profile');
			},

			showDeviceSettings() {
				this.isNotification = false;
				this.isWorkspaceSettings = false;
				this.isProfile = false;
				this.isDevice = true;
				this.updateQueryParam('device');
			},

			updateQueryParam(tab) {
				this.$router
					.push({
						query: { ...this.$route.query, tab },
					})
					.catch(() => {});
			},

			async generateSmartDeviceToken() {
				this.showConfirm(
					'Generate New Token',
					this.user.smart_device_token
						? 'Are you sure you want to generate a new token? This will invalidate the existing token and any devices using it will need to be updated.'
						: 'Generate a new token for your smart devices?',
					async () => {
						try {
							const response = await generateSmartDeviceToken();
							this.user = await getUser();
							this.confirm = null;
							this.showAlert('Token generated successfully');
						} catch (error) {
							console.error('Failed to generate token:', error);
							this.showAlert('Failed to generate token', 'error');
						}
					},
				);
			},

			async revokeSmartDeviceToken() {
				this.showConfirm(
					'Revoke Token',
					'Are you sure you want to revoke this token? All devices using this token will stop working until reconfigured with a new token.',
					async () => {
						try {
							await revokeSmartDeviceToken();
							this.user = await getUser();
							this.confirm = null;
							this.showAlert('Token revoked successfully');
						} catch (error) {
							console.error('Failed to revoke token:', error);
							this.showAlert('Failed to revoke token', 'error');
						}
					},
				);
			},

			showAlert(message, type = 'success') {
				// Implement your alert system here
				// This could be a toast notification or any other alert mechanism
				console.log(`${type}: ${message}`);
			},

			async generateTelegramLink() {
				try {
					const link = await generateLink();

					this.showConfirm(
						'Connect Telegram',
						"Click the button below to open Telegram and connect your account. After connecting, you'll receive notifications through Telegram.",
						() => {
							window.open(link, '_blank');
							this.confirm = null;
						},
						link,
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
					},
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
					// Store original editor value to check if it changes
					const originalEditorSetting = this.settings.find(setting => setting.key === 'preferred_editor');
					const originalEditorValue = originalEditorSetting?.value;
					
					const [data] = await Promise.all([
						updateUserSettingsV2(this.settings),
						updateUserSettings({ settings: this.userSettings }),
					]);
					
					// Find and save preferred_editor to localStorage with improved reliability
					const preferredEditorSetting = this.settings.find(setting => setting.key === 'preferred_editor');
					if (preferredEditorSetting) {
						// Make sure we convert to string and handle null/undefined values
						let editorValue = preferredEditorSetting.value 
							? String(preferredEditorSetting.value).toLowerCase().trim() 
							: '';
						
						// Validate the editor value
						if (editorValue && (editorValue === 'block' || editorValue === 'markdown')) {
							console.log('Saving editor preference to localStorage:', editorValue);
							localStorage.setItem('preferred_editor', editorValue);
							
							// Also update the editor value in the Vuex store
							if (this.$store.state.user && this.$store.state.user.settings) {
								// Update the store's user settings directly 
								this.$store.state.user.settings = this.$store.state.user.settings.map(setting => {
									if (setting.key === 'preferred_editor') {
										return { ...setting, value: editorValue };
									}
									return setting;
								});
							}
							
							// If editor setting has changed, reload the page to apply changes immediately
							const normalizedOriginalValue = originalEditorValue
								? String(originalEditorValue).toLowerCase().trim()
								: '';
								
							if (normalizedOriginalValue !== editorValue) {
								this.showAlert('Editor updated. Reloading page...');
								
								// Force localStorage update one more time to ensure it's saved before reload
								localStorage.setItem('preferred_editor', editorValue);
								
								setTimeout(() => {
									window.location.reload();
								}, 500);
								return;
							}
						} else if (editorValue) {
							console.error('Invalid editor value:', editorValue);
						}
					}
					
					this.initSettings(this.availableSettings, data.settings);
					this.showAlert('Settings updated successfully');
				} catch (e) {
					console.error(e);
					this.showAlert('Failed to update settings', 'error');
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
