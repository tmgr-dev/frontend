<template>
	<Dialog :open="open" @update:open="onOpenChange">
		<DialogContent class="max-h-[85vh] max-w-lg overflow-y-auto">
			<DialogHeader>
				<DialogTitle>What do you want TMGR for?</DialogTitle>
				<DialogDescription>
					You can change this any time in Settings → Modules.
				</DialogDescription>
			</DialogHeader>

			<div class="flex flex-col gap-2" role="radiogroup">
				<label
					v-for="preset in presets"
					:key="preset.key"
					:class="optionClass(preset.key)"
				>
					<input
						v-model="choice"
						type="radio"
						name="modules-choice"
						class="mt-1"
						:value="preset.key"
					/>
					<span class="min-w-0">
						<span class="block text-sm font-medium text-ink">{{
							preset.description || preset.name
						}}</span>
					</span>
				</label>
				<label :class="optionClass(CUSTOM_CHOICE)">
					<input
						v-model="choice"
						type="radio"
						name="modules-choice"
						class="mt-1"
						:value="CUSTOM_CHOICE"
					/>
					<span class="block text-sm font-medium text-ink">Let me pick</span>
				</label>
			</div>

			<div
				v-if="choice === CUSTOM_CHOICE"
				class="flex flex-col gap-4"
				data-testid="modules-picker-list"
			>
				<div v-for="pack in packs" :key="pack.key">
					<h4
						class="mb-1 text-xs font-medium uppercase tracking-wide text-ink-subtle"
					>
						{{ pack.name }}
					</h4>
					<div class="divide-y divide-border">
						<div
							v-for="module in pack.modules"
							:key="module.key"
							class="flex items-center justify-between gap-3 py-2"
						>
							<div class="min-w-0">
								<p class="text-sm text-ink">{{ module.name }}</p>
								<p class="text-xs text-ink-subtle">{{ module.description }}</p>
							</div>
							<Switch
								:checked="custom[module.key]"
								@update:checked="(value) => (custom[module.key] = value)"
							/>
						</div>
					</div>
				</div>
			</div>

			<p v-if="error" class="text-sm text-destructive">{{ error }}</p>

			<DialogFooter>
				<Button variant="ghost" :disabled="saving" @click="dismiss">
					Not now
				</Button>
				<Button :disabled="saving || !choice" @click="save">
					{{ saving ? 'Saving…' : 'Continue' }}
				</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>

<script setup lang="ts">
	import { saveModulesChoice } from '@/actions/tmgr/modules';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { Switch } from '@/components/ui/switch';
	import {
		buildChoiceRequest,
		CUSTOM_CHOICE,
		dismissPicker,
		groupModules,
		isPickerDismissed,
		shouldShowPicker,
	} from '@/utils/modules';
	import { computed, reactive, ref, watch } from 'vue';
	import { useStore } from 'vuex';

	const store = useStore();

	const open = ref(false);
	const choice = ref('');
	const saving = ref(false);
	const error = ref('');
	const custom = reactive<Record<string, boolean>>({});

	const toggles = computed(() => (store.state as any).featureToggles);
	const meta = computed(() => toggles.value.modulesMeta);
	const workspaceId = computed(() => store.getters.currentWorkspaceId);
	const presets = computed(() => meta.value?.presets ?? []);
	const packs = computed(() => {
		const grouped = groupModules({
			configured: false,
			canManage: true,
			enforcement: 'report',
			packs: meta.value?.packs ?? [],
			presets: [],
			modules: Object.values(toggles.value.workspaceToggles),
		} as any);
		return grouped.packs;
	});

	const eligible = computed(
		() =>
			!!meta.value &&
			!isPickerDismissed(workspaceId.value) &&
			shouldShowPicker({
				configured: meta.value.configured,
				canManage: meta.value.canManage,
				isOwner: meta.value.canManage,
				isLocal: !!store.getters.currentWorkspace?.is_local,
				notFound: false,
			}),
	);

	watch(
		[eligible, workspaceId],
		([show]) => {
			open.value = show;
			if (show) {
				choice.value = presets.value[0]?.key ?? CUSTOM_CHOICE;
				error.value = '';
				for (const pack of packs.value)
					for (const module of pack.modules)
						custom[module.key] = module.enabled === true;
			}
		},
		{ immediate: true },
	);

	const optionClass = (key: string) => [
		'flex cursor-pointer items-start gap-3 rounded-md border p-3 transition-colors',
		choice.value === key
			? 'border-primary bg-primary/5'
			: 'border-border hover:bg-muted/50',
	];

	const dismiss = () => {
		dismissPicker(workspaceId.value);
		open.value = false;
	};

	const onOpenChange = (value: boolean) => {
		if (!value) dismiss();
	};

	const save = async () => {
		saving.value = true;
		error.value = '';
		try {
			const payload = await saveModulesChoice(
				workspaceId.value,
				buildChoiceRequest(choice.value, { ...custom }),
			);
			store.commit('featureToggles/setWorkspaceModules', {
				workspaceId: workspaceId.value,
				payload,
			});
			open.value = false;
		} catch {
			error.value = 'Could not save your choice. Try again.';
		} finally {
			saving.value = false;
		}
	};
</script>
