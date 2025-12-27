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
	border: 1px solid rgba(0, 0, 0, 0.08);
	border-radius: 0.75rem;
	background: linear-gradient(135deg, rgba(255, 255, 255, 0.5) 0%, rgba(255, 255, 255, 0.1) 100%);
	backdrop-filter: blur(10px);
	transition: all 0.3s ease;

	&:hover {
		border-color: rgba(102, 126, 234, 0.2);
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.05);
	}

	.dark & {
		border-color: rgba(255, 255, 255, 0.08);
		background: linear-gradient(135deg, rgba(255, 255, 255, 0.05) 0%, rgba(255, 255, 255, 0.02) 100%);
		
		&:hover {
			border-color: rgba(102, 126, 234, 0.3);
			box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
		}
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
	background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
	-webkit-background-clip: text;
	-webkit-text-fill-color: transparent;
	background-clip: text;

	.dark & {
		background: linear-gradient(135deg, #818cf8 0%, #a78bfa 100%);
		-webkit-background-clip: text;
		-webkit-text-fill-color: transparent;
		background-clip: text;
	}
}

.section-description {
	font-size: 0.875rem;
	color: rgba(0, 0, 0, 0.65);
	margin-bottom: 1rem;
	line-height: 1.5;

	&.warning {
		color: #f59e0b;
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem;
		background-color: rgba(245, 158, 11, 0.08);
		border-radius: 0.375rem;
		font-weight: 500;

		&:before {
			content: '⚠️';
		}
	}

	.dark & {
		color: rgba(255, 255, 255, 0.7);

		&.warning {
			color: #fbbf24;
			background-color: rgba(251, 191, 36, 0.1);
		}
	}
}

.toggle-switch {
	position: relative;
	display: inline-block;
	width: 52px;
	height: 28px;
	cursor: pointer;

	input {
		opacity: 0;
		width: 0;
		height: 0;

		&:checked + .toggle-slider {
			background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
			box-shadow: 0 2px 8px rgba(102, 126, 234, 0.4);

			&:before {
				transform: translateX(24px);
				box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
			}
		}

		&:disabled + .toggle-slider {
			opacity: 0.4;
			cursor: not-allowed;
		}

		&:focus + .toggle-slider {
			box-shadow: 0 0 0 3px rgba(102, 126, 234, 0.2);
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
	background-color: #e2e8f0;
	transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
	border-radius: 28px;
	box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.1);

	.dark & {
		background-color: #4a5568;
		box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.3);
	}

	&:before {
		position: absolute;
		content: '';
		height: 22px;
		width: 22px;
		left: 3px;
		bottom: 3px;
		background-color: white;
		transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
		border-radius: 50%;
		box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);

		.dark & {
			background-color: #f7fafc;
		}
	}

	&:hover {
		background-color: #cbd5e0;

		.dark & {
			background-color: #5a6778;
		}
	}
}

.notification-types {
	display: flex;
	flex-direction: column;
	gap: 0.5rem;
	padding: 1rem;
	background-color: rgba(0, 0, 0, 0.02);
	border-radius: 0.5rem;
	margin-top: 0.75rem;

	.dark & {
		background-color: rgba(255, 255, 255, 0.03);
	}
}

.type-checkbox {
	display: flex;
	align-items: center;
	gap: 0.75rem;
	cursor: pointer;
	padding: 0.75rem;
	border-radius: 0.5rem;
	transition: all 0.2s ease;
	border: 1px solid transparent;

	&:hover {
		background-color: rgba(102, 126, 234, 0.05);
		border-color: rgba(102, 126, 234, 0.1);
	}

	.dark & {
		&:hover {
			background-color: rgba(102, 126, 234, 0.1);
			border-color: rgba(102, 126, 234, 0.2);
		}
	}

	input[type='checkbox'] {
		width: 20px;
		height: 20px;
		cursor: pointer;
		accent-color: #667eea;
		border-radius: 0.25rem;
		transition: all 0.2s ease;

		&:hover {
			transform: scale(1.05);
		}
	}

	span {
		font-size: 0.9375rem;
		font-weight: 500;
		color: rgba(0, 0, 0, 0.85);

		.dark & {
			color: rgba(255, 255, 255, 0.9);
		}
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
	font-weight: 500;
	color: rgba(0, 0, 0, 0.6);
	border-radius: 0.5rem;
	transition: all 0.3s ease;

	&.success {
		color: #10b981;
		background-color: rgba(16, 185, 129, 0.08);
		animation: fadeIn 0.3s ease-in;
	}

	.dark & {
		color: rgba(255, 255, 255, 0.6);

		&.success {
			color: #34d399;
			background-color: rgba(52, 211, 153, 0.1);
		}
	}
}

@keyframes fadeIn {
	from {
		opacity: 0;
		transform: translateY(-4px);
	}
	to {
		opacity: 1;
		transform: translateY(0);
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

