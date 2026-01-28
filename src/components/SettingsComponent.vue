<template>
	<Dialog>
		<DialogTrigger as-child>
			<button type="button" title="Settings">
				<CogIcon
					class="size-6 fill-gray-400 transition hover:fill-black dark:hover:fill-white"
				/>
			</button>
		</DialogTrigger>

		<DialogContent
			class="!rounded-[8px] bg-white dark:border-transparent dark:bg-gray-900 dark:text-white sm:max-w-[425px]"
		>
			<DialogHeader>
				<DialogTitle>Settings</DialogTitle>
				<DialogDescription>
					Make changes to the task settings here. Click save when you're done.
				</DialogDescription>
			</DialogHeader>

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

				<div v-for="setting in settings" :key="setting.id" id="settings" class="mt-4">
					<label
						:for="`setting-${setting.id}`"
						class="mb-2 block text-left text-sm font-bold"
					>
						{{ setting.name }}
					</label>

					<div class="relative mb-4">
						<TimeField
							v-if="setting.component_type === 'time_in_seconds'"
							v-model="setting.value"
							:placeholder="setting.description"
						/>
						<template v-else-if="setting.component_type === 'select'">
							<Select
								v-model="setting.value"
								:options="
									setting.default_values.map((val) => ({
										label: val.value,
										value: val.value,
									}))
								"
								:placeholder="setting.description"
							/>
						</template>
						<TextField
							v-else
							v-model="setting.value"
							:placeholder="setting.description"
						/>
					</div>
				</div>
			</div>

			<DialogFooter>
				<DialogClose as-child>
					<button
						type="button"
						class="w-full rounded bg-tmgr-light-blue p-2 text-white hover:opacity-90"
						@click="handleSave"
					>
						Save
					</button>
				</DialogClose>
			</DialogFooter>
		</DialogContent>
	</Dialog>
	<!--	<form
		v-if="settings.length > 0"
		@submit.prevent="saveSettings"
		class="text-gray-800 dark:text-tmgr-gray"
	>
		<button @click="emit('close')" class="absolute right-6 top-7">
			<XMarkIcon
				class="size-5 fill-neutral-600 hover:fill-black dark:hover:fill-white"
			/>
		</button>

		<button
			type="submit"
			class="w-full rounded bg-tmgr-light-blue p-2 text-white"
		>
			Update
		</button>
	</form>-->
</template>

<script setup lang="ts">
	import TimeField from '@/components/general/TimeField.vue';
	import TextField from '@/components/general/TextField.vue';
	import {
		getTaskSettings,
		Setting,
		SettingPayload,
		updateTaskSettings,
	} from '@/actions/tmgr/settings';
	import { onBeforeMount, ref, watch } from 'vue';
	import { Task } from '@/actions/tmgr/tasks';
	import { CogIcon } from '@heroicons/vue/20/solid';
	import {
		Dialog,
		DialogClose,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
		DialogTrigger,
	} from '@/components/ui/dialog';
	import Select from '@/components/general/Select.vue';

	const emit = defineEmits(['close']);

	interface SettingWithValue extends Setting {
		value: any;
	}

	interface Props {
		form: Task;
	}

	const props = defineProps<Props>();
	const settings = ref<SettingWithValue[]>([]);
	const settingsSchema = ref<Setting[]>([]);

	onBeforeMount(async () => {
		settingsSchema.value = await getTaskSettings();
		mergeSettingsWithFormValues();
	});

	watch(
		() => props.form.settings,
		() => {
			mergeSettingsWithFormValues();
		},
		{ deep: true }
	);

	function mergeSettingsWithFormValues() {
		if (!settingsSchema.value.length) return;
		
		settings.value = settingsSchema.value.map((setting: Setting) => {
			const taskSetting = props.form.settings?.find(
				(s: any) => s.id === setting.id || s.key === setting.key
			) as any;
			
			return {
				...setting,
				value: taskSetting?.value ?? taskSetting?.pivot?.value ?? '',
			};
		});
	}

	async function handleSave() {
		if (!props.form.id || !settings.value.length) return;
		
		try {
			const payload: SettingPayload[] = settings.value.map((setting) => ({
				id: setting.id,
				value: setting.value,
			}));
			await updateTaskSettings(props.form.id, payload);
		} catch (e) {
			console.error('Failed to save settings:', e);
		}
	}
</script>
