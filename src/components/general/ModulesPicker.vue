<template>
	<Dialog :open="open" @update:open="onOpenChange">
		<DialogContent class="max-h-[85vh] max-w-lg overflow-y-auto">
			<DialogHeader>
				<DialogTitle>What do you want TMGR for?</DialogTitle>
				<DialogDescription>
					You can change this any time in Settings → Modules.
				</DialogDescription>
			</DialogHeader>

			<div class="grid gap-3 sm:grid-cols-[minmax(0,1fr)_200px]">
				<ModulePresetOptions
					v-model="choice"
					:presets="presets"
					:catalog="catalog"
					show-custom
					@hover="hovered = $event"
				/>
				<ModulePresetPreview v-if="previewPreset" v-bind="preview" />
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
	import ModulePresetOptions from '@/components/general/ModulePresetOptions.vue';
	import ModulePresetPreview from '@/components/general/ModulePresetPreview.vue';
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
	import { presetTarget, previewSurfaces } from '@/utils/previewSurfaces';
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

	const catalog = computed(
		() => Object.values(toggles.value.workspaceToggles) as any[],
	);
	const hovered = ref<string | null>(null);
	const previewPreset = computed(
		() =>
			presets.value.find(
				(p: any) => p.key === (hovered.value ?? choice.value),
			) ?? null,
	);
	const preview = computed(() =>
		previewSurfaces(presetTarget(previewPreset.value as any, catalog.value)),
	);

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
