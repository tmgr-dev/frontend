<template>
	<div class="flex flex-col gap-6">
		<div v-if="loading" class="text-sm text-ink-subtle">Loading settings…</div>

		<div v-else-if="error" class="flex flex-col items-start gap-2">
			<p class="text-sm text-destructive">{{ error }}</p>
			<Button variant="outline" size="sm" @click="loadSettings">
				Try again
			</Button>
		</div>

		<form v-else class="flex flex-col gap-6" @submit.prevent="handleSubmit">
			<SettingsSection
				title="Web notifications"
				description="Receive browser notifications for workspace changes"
			>
				<template #actions>
					<Switch
						:checked="formData.web_enabled"
						@update:checked="onWebEnabledChange"
					/>
				</template>
				<div
					v-if="formData.web_enabled"
					class="flex flex-col divide-y divide-border rounded-md border border-border"
				>
					<label
						v-for="type in notificationTypeGroups"
						:key="type.id"
						class="flex items-center gap-3 px-3 py-2 text-sm text-ink"
					>
						<input
							type="checkbox"
							:value="type.id"
							v-model="formData.web_types"
							class="h-4 w-4 rounded border-border accent-primary focus:ring-ring"
						/>
						{{ type.label }}
					</label>
				</div>
			</SettingsSection>

			<SettingsSection title="Telegram notifications">
				<template #description>
					<span v-if="!hasTelegram">
						Connect your Telegram account in profile settings to receive
						notifications
					</span>
					<span v-else>Send notifications to Telegram bot</span>
				</template>
				<template #actions>
					<Switch
						:checked="formData.telegram_enabled"
						:disabled="!hasTelegram"
						@update:checked="onTelegramEnabledChange"
					/>
				</template>
				<div
					v-if="formData.telegram_enabled && hasTelegram"
					class="flex flex-col divide-y divide-border rounded-md border border-border"
				>
					<label
						v-for="type in notificationTypeGroups"
						:key="type.id"
						class="flex items-center gap-3 px-3 py-2 text-sm text-ink"
					>
						<input
							type="checkbox"
							:value="type.id"
							v-model="formData.telegram_types"
							class="h-4 w-4 rounded border-border accent-primary focus:ring-ring"
						/>
						{{ type.label }}
					</label>
				</div>
			</SettingsSection>

			<SettingsSection
				title="Email notifications"
				description="Receive email notifications for important changes"
			>
				<template #actions>
					<Switch
						:checked="formData.email_enabled"
						@update:checked="onEmailEnabledChange"
					/>
				</template>
				<div
					v-if="formData.email_enabled"
					class="flex flex-col divide-y divide-border rounded-md border border-border"
				>
					<label
						v-for="type in notificationTypeGroups"
						:key="type.id"
						class="flex items-center gap-3 px-3 py-2 text-sm text-ink"
					>
						<input
							type="checkbox"
							:value="type.id"
							v-model="formData.email_types"
							class="h-4 w-4 rounded border-border accent-primary focus:ring-ring"
						/>
						{{ type.label }}
					</label>
				</div>
			</SettingsSection>

			<SettingsSection
				title="Agents"
				description="Notifications Claude Code, Codex or other AI agents send you with a notify token"
			>
				<div class="flex flex-col divide-y divide-border rounded-md border border-border">
					<div class="flex items-center justify-between gap-3 px-3 py-2 text-sm text-ink">
						Push to the mobile app
						<Switch
							:checked="formData.agent_push_enabled"
							@update:checked="(v) => (formData.agent_push_enabled = v)"
						/>
					</div>
					<div class="flex items-center justify-between gap-3 px-3 py-2 text-sm text-ink">
						Telegram
						<Switch
							:checked="formData.agent_telegram_enabled"
							@update:checked="(v) => (formData.agent_telegram_enabled = v)"
						/>
					</div>
				</div>
			</SettingsSection>

			<div class="flex justify-end">
				<span v-if="saving" class="text-xs text-ink-subtle">Saving…</span>
				<span v-else class="text-xs text-status-done">Saved</span>
			</div>
		</form>
	</div>
</template>

<script>
	import {
		getNotificationSettings,
		updateNotificationSettings,
	} from '@/actions/tmgr/notifications';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import { Switch } from '@/components/ui/switch';
	import {
		computed,
		defineComponent,
		onMounted,
		reactive,
		ref,
		watch,
	} from 'vue';
	import { notificationTypeLabel } from '@/utils/notificationTypes';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'NotificationSettingsForm',
		components: { SettingsSection, Button, Switch },
		setup() {
			const store = useStore();
			const loading = ref(true);
			const saving = ref(false);
			const error = ref(null);

			const formData = reactive({
				web_enabled: false,
				web_types: [],
				telegram_enabled: false,
				telegram_types: [],
				email_enabled: true,
				email_types: [],
				agent_push_enabled: true,
				agent_telegram_enabled: false,
			});

			const availableTypes = ref([]);

			const notificationTypeGroups = computed(() => {
				return availableTypes.value.map((type) => ({
					id: type,
					label: notificationTypeLabel(type),
				}));
			});

			const hasTelegram = computed(() => {
				const user = store.getters['user/getUser'];
				return user?.telegram_id != null || user?.telegram_username != null;
			});

			const loadSettings = async () => {
				try {
					loading.value = true;
					error.value = null;

					const response = await getNotificationSettings();

					formData.web_enabled = response.settings.web_enabled;
					formData.web_types = response.settings.web_types || [];
					formData.telegram_enabled = response.settings.telegram_enabled;
					formData.telegram_types = response.settings.telegram_types || [];
					formData.email_enabled = response.settings.email_enabled;
					formData.email_types = response.settings.email_types || [];
					formData.agent_push_enabled =
						response.settings.agent_push_enabled ?? true;
					formData.agent_telegram_enabled =
						response.settings.agent_telegram_enabled ?? false;

					availableTypes.value = response.available_types;
				} catch (err) {
					console.error('Error loading notification settings:', err);
					error.value =
						err.response?.data?.message || 'Failed to load settings';
				} finally {
					loading.value = false;
				}
			};

			const handleWebEnabledChange = () => {
				if (!formData.web_enabled) {
					formData.web_types = [];
				} else if (formData.web_types.length === 0) {
					formData.web_types = [...availableTypes.value];
				}
			};

			const handleTelegramEnabledChange = () => {
				if (!formData.telegram_enabled) {
					formData.telegram_types = [];
				} else if (formData.telegram_types.length === 0) {
					formData.telegram_types = [
						'task_created',
						'task_updated',
						'task_status_changed',
						'task_assigned',
						'comment_created',
						'file_uploaded',
						'member_joined',
					];
				}
			};

			const handleEmailEnabledChange = () => {
				if (!formData.email_enabled) {
					formData.email_types = [];
				} else if (formData.email_types.length === 0) {
					formData.email_types = [
						'task_assigned',
						'task_status_changed',
						'comment_created',
						'member_joined',
					];
				}
			};

			const onWebEnabledChange = (value) => {
				formData.web_enabled = value;
				handleWebEnabledChange();
			};

			const onTelegramEnabledChange = (value) => {
				formData.telegram_enabled = value;
				handleTelegramEnabledChange();
			};

			const onEmailEnabledChange = (value) => {
				formData.email_enabled = value;
				handleEmailEnabledChange();
			};

			const handleSubmit = async () => {
				try {
					saving.value = true;
					error.value = null;

					await updateNotificationSettings({
						web_enabled: formData.web_enabled,
						web_types:
							formData.web_types.length > 0 ? formData.web_types : null,
						telegram_enabled: formData.telegram_enabled,
						telegram_types:
							formData.telegram_types.length > 0
								? formData.telegram_types
								: null,
						email_enabled: formData.email_enabled,
						email_types:
							formData.email_types.length > 0 ? formData.email_types : null,
						agent_push_enabled: formData.agent_push_enabled,
						agent_telegram_enabled: formData.agent_telegram_enabled,
					});
				} catch (err) {
					console.error('Error saving notification settings:', err);
					error.value =
						err.response?.data?.message || 'Failed to save settings';
				} finally {
					saving.value = false;
				}
			};

			let saveTimeout = null;
			watch(
				formData,
				() => {
					if (saveTimeout) {
						clearTimeout(saveTimeout);
					}
					saveTimeout = setTimeout(() => {
						handleSubmit();
					}, 800);
				},
				{ deep: true },
			);

			onMounted(() => {
				loadSettings();
			});

			return {
				loading,
				saving,
				error,
				formData,
				notificationTypeGroups,
				hasTelegram,
				loadSettings,
				onWebEnabledChange,
				onTelegramEnabledChange,
				onEmailEnabledChange,
				handleSubmit,
			};
		},
	});
</script>
