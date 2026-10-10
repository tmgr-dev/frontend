<template>
	<div class="flex flex-col gap-1">
		<p class="mb-3 text-sm text-muted-foreground">
			Global shortcuts work from any app, even when TMGR is hidden. Click a
			shortcut and press a new combination; Esc cancels.
		</p>
		<div
			v-for="action in actions"
			:key="action.id"
			class="flex flex-wrap items-center gap-3 rounded-md px-2 py-2 hover:bg-muted/50 dark:hover:bg-muted/30"
		>
			<div class="min-w-[200px] flex-1">
				<div class="text-sm font-medium">{{ action.label }}</div>
				<div class="text-xs text-muted-foreground">{{ action.hint }}</div>
			</div>
			<Button
				variant="outline"
				size="sm"
				:class="[
					'min-w-[120px] font-mono',
					recording === action.id && 'border-primary text-primary',
					!config[action.id].enabled && 'opacity-50',
				]"
				@click="startRecording(action.id)"
				@keydown="(event) => onKey(action.id, event)"
				@blur="stopRecording"
			>
				{{
					recording === action.id
						? 'Press keys…'
						: describe(config[action.id].accelerator)
				}}
			</Button>
			<Button
				variant="ghost"
				size="sm"
				title="Reset to default"
				@click="reset(action.id)"
			>
				<RotateCcw class="h-4 w-4" />
			</Button>
			<Switch
				:checked="config[action.id].enabled"
				@update:checked="(value) => setEnabled(action.id, value)"
			/>
			<div class="w-full pl-0 text-xs md:w-auto md:pl-2">
				<span v-if="errors[action.id]" class="text-destructive">
					{{ errors[action.id] }}
				</span>
				<span
					v-else-if="status[action.id] === 'taken'"
					class="text-destructive"
				>
					Taken by another app — choose a different shortcut
				</span>
			</div>
		</div>
	</div>
</template>

<script>
	import { Button } from '@/components/ui/button';
	import { Switch } from '@/components/ui/switch';
	import {
		DEFAULT_SHORTCUTS,
		describeAccelerator,
		eventToAccelerator,
		findConflict,
		primaryModifiersHint,
		shortcutActionsFor,
		saveShortcuts,
		shortcutConfig,
		shortcutStatus,
		validateAccelerator,
	} from '@/utils/desktopShortcuts';
	import { isMacLike, desktopPlatform } from '@/utils/desktop';
	import { RotateCcw } from 'lucide-vue-next';
	import { computed, defineComponent, reactive, ref } from 'vue';

	const ACTIONS = [
		{
			id: 'quickAdd',
			label: 'Quick add',
			hint: 'Opens a small window to add a Daily Routine',
		},
		{
			id: 'timer',
			label: 'Start / stop timer',
			hint: 'Stops running timers, or restarts the last task',
		},
		{
			id: 'screenshot',
			label: 'Screenshot to task',
			hint: isMacLike(desktopPlatform())
				? 'Select an area; creates a backlog task with the image (needs Screen Recording)'
				: 'Select an area; creates a backlog task with the image',
		},
		{
			id: 'selection',
			label: 'Selected text to task',
			hint: 'Copies the selection from any app; needs Accessibility permission',
		},
	];

	const LABELS = Object.fromEntries(ACTIONS.map((a) => [a.id, a.label]));

	export default defineComponent({
		name: 'DesktopShortcutsSettings',
		components: { RotateCcw, Switch, Button },
		setup() {
			const recording = ref(null);
			const errors = reactive({});
			const config = computed(() => shortcutConfig.value);

			const update = (action, patch) => {
				saveShortcuts({
					...shortcutConfig.value,
					[action]: { ...shortcutConfig.value[action], ...patch },
				});
			};

			const onKey = (action, event) => {
				if (recording.value !== action) return;
				event.preventDefault();
				event.stopPropagation();
				if (event.key === 'Escape') {
					recording.value = null;
					return;
				}
				const accelerator = eventToAccelerator(event);
				if (!accelerator) return;
				const invalid = validateAccelerator(accelerator);
				if (invalid) {
					errors[action] =
						invalid === 'reserved'
							? 'This is a system shortcut'
							: primaryModifiersHint();
					return;
				}
				const conflict = findConflict(shortcutConfig.value, action, accelerator);
				if (conflict) {
					errors[action] = `Already used for “${LABELS[conflict]}”`;
					return;
				}
				errors[action] = '';
				recording.value = null;
				update(action, { accelerator, enabled: true });
			};

			return {
				actions: ACTIONS.filter((a) => shortcutActionsFor().includes(a.id)),
				config,
				status: computed(() => shortcutStatus.value),
				recording,
				errors,
				describe: describeAccelerator,
				startRecording: (action) => {
					errors[action] = '';
					recording.value = action;
				},
				stopRecording: () => {
					recording.value = null;
				},
				onKey,
				reset: (action) => {
					errors[action] = '';
					update(action, { ...DEFAULT_SHORTCUTS[action] });
				},
				setEnabled: (action, enabled) => update(action, { enabled }),
			};
		},
	});
</script>
