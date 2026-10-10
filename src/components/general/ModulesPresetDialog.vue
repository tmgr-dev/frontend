<template>
	<Dialog :open="open" @update:open="(value) => !value && close()">
		<DialogContent class="max-h-[85vh] max-w-2xl overflow-y-auto">
			<DialogHeader>
				<DialogTitle>Apply a preset</DialogTitle>
				<DialogDescription>
					Pick a preset to see what changes before anything is applied.
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
				<p v-if="isEmpty" data-testid="preset-diff-empty">Nothing changes</p>
				<template v-else>
					<p v-if="diff.on.length" data-testid="preset-diff-on">
						<span class="font-medium">Will turn on:</span>
						{{ diff.on.join(', ') }}
					</p>
					<p v-if="diff.off.length" data-testid="preset-diff-off">
						<span class="font-medium">Will turn off:</span>
						{{ diff.off.join(', ') }}
					</p>
				</template>
				<p class="text-xs text-ink-subtle">
					Your data is never deleted. Turning a module back on brings everything
					back.
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
					Cancel
				</Button>
				<Button
					:disabled="saving || !selected || isEmpty"
					data-testid="preset-confirm"
					@click="apply"
				>
					{{ saving ? 'Applying…' : 'Confirm' }}
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
	import {
		buildChoiceRequest,
		presetDiff,
		type ModuleEntry,
		type ModulePreset,
		type ModulesPayload,
	} from '@/utils/modules';
	import { presetTarget, previewSurfaces } from '@/utils/previewSurfaces';
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
	const isEmpty = computed(
		() => !diff.value.on.length && !diff.value.off.length,
	);
	const currentOn = (key: string) =>
		props.catalog.find((m) => m.key === key)?.enabled === true ||
		props.catalog.find((m) => m.key === key)?.core === true;
	const preview = computed(() =>
		previewSurfaces(
			presetTarget(shown.value as ModulePreset, props.catalog),
			currentOn,
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
			emit('applied', payload);
			close();
		} catch {
			error.value = 'Could not apply the preset. Try again.';
		} finally {
			saving.value = false;
		}
	};
</script>
