<template>
	<Dialog :open="open" @update:open="(value) => !value && close()">
		<DialogContent class="max-h-[85vh] max-w-2xl overflow-y-auto">
			<DialogHeader>
				<DialogTitle>{{ $t('general.modulesPresetDialog.title') }}</DialogTitle>
				<DialogDescription>
					{{ $t('general.modulesPresetDialog.description') }}
				</DialogDescription>
			</DialogHeader>

			<div class="grid gap-3 sm:grid-cols-[minmax(0,1fr)_200px]">
				<ModulePresetOptions
					v-model="choice"
					:presets="presets"
					:catalog="catalog"
					@hover="hovered = $event"
				/>
				<ModulePresetPreview v-if="selected" v-bind="preview" />
			</div>

			<div v-if="selected" class="flex flex-col gap-2 text-sm">
				<p v-if="isEmpty" data-testid="preset-diff-empty">
					{{ $t('general.modulesPresetDialog.nothingChanges') }}
				</p>
				<template v-else>
					<p v-if="diff.on.length" data-testid="preset-diff-on">
						<span class="font-medium">{{
							$t('general.modulesPresetDialog.willTurnOn')
						}}</span>
						{{ diff.on.join(', ') }}
					</p>
					<p v-if="diff.off.length" data-testid="preset-diff-off">
						<span class="font-medium">{{
							$t('general.modulesPresetDialog.willTurnOff')
						}}</span>
						{{ diff.off.join(', ') }}
					</p>
					<p v-for="note in notes" :key="note" data-testid="preset-diff-note">
						{{ note }}
					</p>
				</template>
				<p class="text-xs text-ink-subtle">
					{{ $t('general.modulesPresetDialog.dataKept') }}
				</p>
			</div>

			<p v-if="error" class="text-sm text-destructive">{{ error }}</p>

			<DialogFooter>
				<Button
					variant="ghost"
					:disabled="saving"
					data-testid="preset-cancel"
					@click="close"
				>
					{{ $t('common.actions.cancel') }}
				</Button>
				<Button
					:disabled="saving || !selected || isEmpty"
					data-testid="preset-confirm"
					@click="apply"
				>
					{{
						saving
							? $t('general.modulesPresetDialog.applying')
							: $t('common.actions.confirm')
					}}
				</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>

<script setup lang="ts">
	import { t } from '@/i18n';
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
	import { useToast } from '@/components/ui/toast';
	import {
		buildChoiceRequest,
		payloadNotices,
		presetDiff,
		presetNotes,
		type ModuleEntry,
		type ModulePreset,
		type ModulesPayload,
	} from '@/utils/modules';
	import {
		currentTarget,
		presetTarget,
		previewSurfaces,
	} from '@/utils/previewSurfaces';
	import { computed, ref, watch } from 'vue';

	const props = defineProps<{
		open: boolean;
		workspaceId: number | string;
		presets: ModulePreset[];
		catalog: ModuleEntry[];
	}>();
	const emit = defineEmits<{
		'update:open': [value: boolean];
		applied: [payload: ModulesPayload];
	}>();

	const toaster = useToast();
	const choice = ref('');
	const hovered = ref<string | null>(null);
	const saving = ref(false);
	const error = ref('');

	watch(
		() => props.open,
		(open) => {
			if (!open) return;
			choice.value = '';
			hovered.value = null;
			error.value = '';
		},
	);

	const selected = computed(
		() => props.presets.find((p) => p.key === choice.value) ?? null,
	);
	const shown = computed(
		() =>
			props.presets.find((p) => p.key === (hovered.value ?? choice.value)) ??
			null,
	);
	const diff = computed(() =>
		selected.value
			? presetDiff(selected.value, props.catalog)
			: { on: [], off: [] },
	);
	const notes = computed(() =>
		selected.value ? presetNotes(selected.value, props.catalog) : [],
	);
	const isEmpty = computed(
		() => !diff.value.on.length && !diff.value.off.length,
	);
	const preview = computed(() =>
		previewSurfaces(
			presetTarget(shown.value as ModulePreset, props.catalog),
			currentTarget(props.catalog),
		),
	);

	const close = () => emit('update:open', false);

	const apply = async () => {
		if (!selected.value) return;
		saving.value = true;
		error.value = '';
		try {
			const payload = await saveModulesChoice(
				props.workspaceId,
				buildChoiceRequest(selected.value.key, {}),
			);
			payloadNotices(payload).forEach((title) => toaster.toast({ title }));
			emit('applied', payload);
			close();
		} catch {
			error.value = t('general.modulesPresetDialog.applyFailed');
		} finally {
			saving.value = false;
		}
	};
</script>
