<template>
	<div
		class="flex min-h-0 flex-1 flex-col gap-2"
		data-testid="move-routines-picker"
	>
		<div class="flex flex-wrap items-center gap-x-3 gap-y-1 px-1 text-xs">
			<label class="flex items-center gap-2 text-ink">
				<input
					type="checkbox"
					class="size-4 accent-brand"
					:checked="allSelected"
					:disabled="!routines.length"
					aria-label="Select all local routines"
					@change="toggleAll"
				/>
				Select all
			</label>
			<span class="text-ink-subtle"
				>Pick the local routines to send to a cloud workspace.</span
			>
		</div>
		<p v-if="error" role="alert" class="px-1 text-sm text-destructive">
			{{ error }}
		</p>
		<p
			v-else-if="loaded && !routines.length"
			class="px-1 py-6 text-center text-sm text-ink-subtle"
		>
			There are no local routines to move.
		</p>
		<ul class="flex min-h-0 flex-1 flex-col gap-1.5 overflow-y-auto pb-2">
			<li v-for="r in routines" :key="r.id">
				<label
					class="flex cursor-pointer items-center gap-3 rounded-card border bg-surface px-3.5 py-2.5 transition-colors"
					:class="
						selected.has(r.id)
							? 'border-brand bg-brand-bg'
							: 'border-line hover:border-line-strong'
					"
				>
					<input
						type="checkbox"
						class="size-4 shrink-0 accent-brand"
						:checked="selected.has(r.id)"
						:aria-label="`Select ${r.title}`"
						@change="toggle(r.id)"
					/>
					<span class="min-w-0 flex-1">
						<span class="block truncate text-sm text-ink">{{ r.title }}</span>
						<span
							v-if="r.pattern || r.scheduledDate"
							class="mt-0.5 flex items-center gap-2 text-2xs text-ink-subtle"
						>
							<span v-if="r.pattern" class="capitalize">{{
								r.pattern.frequency.toLowerCase()
							}}</span>
							<span v-if="r.scheduledDate">{{ r.scheduledDate }}</span>
						</span>
					</span>
				</label>
			</li>
		</ul>

		<div
			class="sticky bottom-0 z-10 -mx-2 flex flex-wrap items-center gap-2 border-t border-line bg-surface px-3 py-2.5 md:-mx-5 md:px-5"
			data-testid="move-routines-bar"
		>
			<span class="mr-auto text-sm text-ink" aria-live="polite">
				{{ selected.size }} selected
			</span>
			<button type="button" :class="ghostButton" @click="$emit('cancel')">
				Cancel
			</button>
			<button
				type="button"
				:class="ghostButton"
				:disabled="!selected.size"
				@click="open('tasks')"
			>
				Convert to cloud tasks
			</button>
			<button
				type="button"
				:class="primaryButton"
				:disabled="!selected.size"
				@click="open('routines')"
			>
				Move to cloud routines
			</button>
		</div>

		<MoveRoutinesDialog
			v-if="mode"
			:routines="chosen"
			:mode="mode"
			:targets="targets"
			@close="onDialogClose"
		/>
	</div>
</template>

<script setup lang="ts">
	import type { Workspace } from '@/actions/tmgr/workspaces';
	import { computed, onMounted, ref, type PropType } from 'vue';
	import type { MoveMode } from './batch';
	import { readLocalRoutines } from './localRoutines';
	import MoveRoutinesDialog from './MoveRoutinesDialog.vue';
	import type { LocalRoutine } from './payload';

	defineProps({
		targets: { type: Array as PropType<Workspace[]>, required: true },
	});
	const emit = defineEmits<{
		(e: 'cancel'): void;
		(e: 'changed'): void;
	}>();

	const ghostButton =
		'rounded-md border border-line px-3 py-2 text-sm font-medium text-ink transition-colors hover:bg-surface-hover disabled:cursor-not-allowed disabled:opacity-50';
	const primaryButton =
		'rounded-md bg-brand px-3 py-2 text-sm font-semibold text-on-accent transition-colors hover:bg-brand-hover disabled:cursor-not-allowed disabled:bg-surface-sunken disabled:text-ink-subtle';

	const routines = ref<LocalRoutine[]>([]);
	const selected = ref(new Set<number>());
	const loaded = ref(false);
	const error = ref('');
	const mode = ref<MoveMode | null>(null);
	const chosen = ref<LocalRoutine[]>([]);

	const allSelected = computed(
		() =>
			!!routines.value.length && selected.value.size === routines.value.length,
	);

	const load = async () => {
		try {
			routines.value = await readLocalRoutines();
			error.value = '';
		} catch (e) {
			error.value = 'The local routines could not be read.';
		}
		const ids = new Set(routines.value.map((r) => r.id));
		selected.value = new Set([...selected.value].filter((id) => ids.has(id)));
		loaded.value = true;
	};

	const toggle = (id: number) => {
		const next = new Set(selected.value);
		if (!next.delete(id)) next.add(id);
		selected.value = next;
	};
	const toggleAll = () => {
		selected.value = allSelected.value
			? new Set()
			: new Set(routines.value.map((r) => r.id));
	};

	const open = (next: MoveMode) => {
		chosen.value = routines.value.filter((r) => selected.value.has(r.id));
		mode.value = next;
	};

	const onDialogClose = async (moved: boolean) => {
		mode.value = null;
		if (!moved) return;
		await load();
		emit('changed');
	};

	onMounted(load);
</script>
