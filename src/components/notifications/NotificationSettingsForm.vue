<template>
	<div class="notification-settings-form">
		<div v-if="loading" class="loading-state">
			<div class="spinner"></div>
			<p>Loading settings...</p>
		</div>

		<div v-else-if="error" class="error-state">
			<p>{{ error }}</p>
			<button @click="loadSettings" class="retry-btn">Try again</button>
		</div>

		<form v-else @submit.prevent="handleSubmit" class="settings-form">
			<div class="settings-section">
				<div class="section-header">
					<h3 class="section-title">Web notifications</h3>
					<label class="toggle-switch">
						<input
							type="checkbox"
							v-model="formData.web_enabled"
							@change="handleWebEnabledChange"
						/>
						<span class="toggle-slider"></span>
					</label>
				</div>

				<p class="section-description">
					Receive browser notifications for workspace changes
				</p>

				<div v-if="formData.web_enabled" class="notification-types">
					<label
						v-for="type in notificationTypeGroups"
						:key="type.id"
						class="type-checkbox"
					>
						<input
							type="checkbox"
							:value="type.id"
							v-model="formData.web_types"
						/>
						<span>{{ type.label }}</span>
					</label>
				</div>
			</div>

			<div class="settings-section">
				<div class="section-header">
					<h3 class="section-title">Telegram notifications</h3>
					<label class="toggle-switch">
						<input
							type="checkbox"
							v-model="formData.telegram_enabled"
							:disabled="!hasTelegram"
							@change="handleTelegramEnabledChange"
						/>
						<span class="toggle-slider"></span>
					</label>
				</div>

				<p v-if="!hasTelegram" class="section-description warning">
					Connect your Telegram account in profile settings to receive notifications
				</p>
				<p v-else class="section-description">
					Send notifications to Telegram bot
				</p>

				<div v-if="formData.telegram_enabled && hasTelegram" class="notification-types">
					<label
						v-for="type in notificationTypeGroups"
						:key="type.id"
						class="type-checkbox"
					>
						<input
							type="checkbox"
							:value="type.id"
							v-model="formData.telegram_types"
						/>
						<span>{{ type.label }}</span>
					</label>
				</div>
			</div>

			<div class="settings-section">
				<div class="section-header">
					<h3 class="section-title">Email notifications</h3>
					<label class="toggle-switch">
						<input
							type="checkbox"
							v-model="formData.email_enabled"
							@change="handleEmailEnabledChange"
						/>
						<span class="toggle-slider"></span>
					</label>
				</div>

				<p class="section-description">
					Receive email notifications for important changes
				</p>

				<div v-if="formData.email_enabled" class="notification-types">
					<label
						v-for="type in notificationTypeGroups"
						:key="type.id"
						class="type-checkbox"
					>
						<input
							type="checkbox"
							:value="type.id"
							v-model="formData.email_types"
						/>
						<span>{{ type.label }}</span>
					</label>
				</div>
			</div>

			<div class="form-actions">
				<div v-if="saving" class="auto-save-indicator">
					<div class="spinner-small"></div>
					<span>Saving...</span>
				</div>
				<div v-else class="auto-save-indicator success">
					<span>✓ Auto-saved</span>
				</div>
			</div>
		</form>
	</div>
</template>

<script>
	import { defineComponent, ref, reactive, computed, onMounted, watch } from 'vue';
	import { useStore } from 'vuex';
	import {
		getNotificationSettings,
		updateNotificationSettings,
	} from '@/actions/tmgr/notifications';

export default defineComponent({
	name: 'NotificationSettingsForm',
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
		});

		const availableTypes = ref([]);

		const notificationTypeGroups = computed(() => {
			const typeLabels = {
				task_created: 'Task created',
				task_updated: 'Task updated',
				task_status_changed: 'Task status changed',
				task_assigned: 'Task assigned',
				task_deleted: 'Task deleted',
				task_restored: 'Task restored',
				task_completed: 'Task completed',
				comment_created: 'New comment',
				comment_updated: 'Comment updated',
				comment_deleted: 'Comment deleted',
				category_created: 'Category created',
				category_updated: 'Category updated',
				category_deleted: 'Category deleted',
				category_restored: 'Category restored',
				file_uploaded: 'File uploaded',
				file_deleted: 'File deleted',
				member_joined: 'Member joined',
				member_left: 'Member left workspace',
			};

			return availableTypes.value.map((type) => ({
				id: type,
				label: typeLabels[type] || type,
			}));
		});

		const hasTelegram = computed(() => {
			const user = store.getters['user/getUser'];
			return user?.telegram_id != null;
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

				availableTypes.value = response.available_types;
			} catch (err) {
				console.error('Error loading notification settings:', err);
				error.value = err.response?.data?.message || 'Failed to load settings';
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

		const handleSubmit = async () => {
			try {
				saving.value = true;
				error.value = null;

				await updateNotificationSettings({
					web_enabled: formData.web_enabled,
					web_types: formData.web_types.length > 0 ? formData.web_types : null,
					telegram_enabled: formData.telegram_enabled,
					telegram_types:
						formData.telegram_types.length > 0 ? formData.telegram_types : null,
					email_enabled: formData.email_enabled,
					email_types: formData.email_types.length > 0 ? formData.email_types : null,
				});
			} catch (err) {
				console.error('Error saving notification settings:', err);
				error.value = err.response?.data?.message || 'Failed to save settings';
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
			{ deep: true }
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
			handleWebEnabledChange,
			handleTelegramEnabledChange,
			handleEmailEnabledChange,
			handleSubmit,
		};
	},
});
</script>

<style lang="scss" scoped>
.notification-settings-form {
	max-width: 600px;
}

.loading-state,
.error-state {
	display: flex;
	flex-direction: column;
	align-items: center;
	justify-content: center;
	padding: 3rem;
	gap: 1rem;
}

.spinner {
	width: 48px;
	height: 48px;
	border: 4px solid rgba(0, 0, 0, 0.1);
	border-top-color: var(--tmgr-blue);
	border-radius: 50%;
	animation: spin 0.8s linear infinite;
}

@keyframes spin {
	to {
		transform: rotate(360deg);
	}
}

.retry-btn {
	padding: 0.5rem 1rem;
	background-color: var(--tmgr-blue);
	color: white;
	border: none;
	border-radius: 0.375rem;
	cursor: pointer;
	font-weight: 500;
	transition: opacity 0.2s;

	&:hover {
		opacity: 0.9;
	}
}

.settings-form {
	display: flex;
	flex-direction: column;
	gap: 2rem;
}

.settings-section {
	padding: 1.5rem;
	border: 1px solid rgba(0, 0, 0, 0.1);
	border-radius: 0.5rem;

	.dark & {
		border-color: rgba(255, 255, 255, 0.1);
	}
}

.section-header {
	display: flex;
	align-items: center;
	justify-content: space-between;
	margin-bottom: 0.5rem;
}

.section-title {
	font-size: 1.125rem;
	font-weight: 600;
	margin: 0;
}

.section-description {
	font-size: 0.875rem;
	color: rgba(0, 0, 0, 0.6);
	margin-bottom: 1rem;

	&.warning {
		color: #f59e0b;
	}

	.dark & {
		color: rgba(255, 255, 255, 0.6);

		&.warning {
			color: #fbbf24;
		}
	}
}

.toggle-switch {
	position: relative;
	display: inline-block;
	width: 48px;
	height: 24px;
	cursor: pointer;

	input {
		opacity: 0;
		width: 0;
		height: 0;

		&:checked + .toggle-slider {
			background-color: var(--tmgr-blue);

			&:before {
				transform: translateX(24px);
			}
		}

		&:disabled + .toggle-slider {
			opacity: 0.5;
			cursor: not-allowed;
		}
	}
}

.toggle-slider {
	position: absolute;
	cursor: pointer;
	top: 0;
	left: 0;
	right: 0;
	bottom: 0;
	background-color: #ccc;
	transition: 0.3s;
	border-radius: 24px;

	&:before {
		position: absolute;
		content: '';
		height: 18px;
		width: 18px;
		left: 3px;
		bottom: 3px;
		background-color: white;
		transition: 0.3s;
		border-radius: 50%;
	}
}

.notification-types {
	display: flex;
	flex-direction: column;
	gap: 0.75rem;
	padding-top: 0.5rem;
}

.type-checkbox {
	display: flex;
	align-items: center;
	gap: 0.5rem;
	cursor: pointer;
	padding: 0.5rem;
	border-radius: 0.375rem;
	transition: background-color 0.2s;

	&:hover {
		background-color: rgba(0, 0, 0, 0.02);
	}

	.dark & {
		&:hover {
			background-color: rgba(255, 255, 255, 0.05);
		}
	}

	input[type='checkbox'] {
		width: 18px;
		height: 18px;
		cursor: pointer;
	}

	span {
		font-size: 0.875rem;
	}
}

.form-actions {
	display: flex;
	justify-content: flex-end;
	padding-top: 1rem;
}

.auto-save-indicator {
	display: flex;
	align-items: center;
	gap: 0.5rem;
	padding: 0.5rem 1rem;
	font-size: 0.875rem;
	color: rgba(0, 0, 0, 0.6);

	&.success {
		color: #10b981;
	}

	.dark & {
		color: rgba(255, 255, 255, 0.6);

		&.success {
			color: #34d399;
		}
	}
}

.spinner-small {
	width: 16px;
	height: 16px;
	border: 2px solid rgba(0, 0, 0, 0.1);
	border-top-color: var(--tmgr-blue);
	border-radius: 50%;
	animation: spin 0.8s linear infinite;
}
</style>

