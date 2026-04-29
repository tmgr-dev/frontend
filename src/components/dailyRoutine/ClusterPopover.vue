<template>
	<Teleport to="body">
		<div
			v-if="open"
			class="fixed inset-0 z-[9999]"
			@click.self="$emit('close')"
		>
			<div
				class="absolute min-w-[260px] max-w-[340px] rounded-card border border-line bg-surface p-2 shadow-tmgr-lg"
				:style="popoverStyle"
				@click.stop
			>
				<div class="mb-2 flex items-center justify-between border-b border-line px-2 pb-2">
					<div class="text-2xs font-semibold uppercase tracking-wide text-ink-subtle">
						{{ events.length }} overlapping
					</div>
					<button
						type="button"
						class="flex h-6 w-6 items-center justify-center rounded-pill text-ink-subtle hover:bg-surface-hover hover:text-ink"
						@click="$emit('close')"
					>
						<DRIcon name="close" :size="12" stroke="currentColor" />
					</button>
				</div>
				<div class="flex flex-col gap-1">
					<div
						v-for="ev in events"
						:key="ev.task_id + '-' + ev.date"
						class="group flex items-center gap-2 rounded-md px-2 py-1.5 hover:bg-surface-hover"
					>
						<button
							type="button"
							class="flex h-5 w-5 shrink-0 items-center justify-center rounded-pill border transition-colors"
							:class="
								ev.completed
									? 'border-status-done bg-status-done text-white'
									: 'border-line text-ink-subtle hover:border-status-done'
							"
							@click="$emit('toggle', ev)"
						>
							<DRIcon v-if="ev.completed" name="check" :size="10" stroke="#fff" :sw="2.4" />
						</button>
						<span
							class="h-2 w-2 shrink-0 rounded-pill"
							:style="{ background: ev.routine_category.color }"
						/>
						<span
							class="min-w-0 flex-1 truncate text-xs"
							:class="ev.completed ? 'text-ink-subtle line-through' : 'text-ink'"
						>
							{{ ev.title }}
						</span>
						<span class="text-2xs text-ink-subtle tabular-nums">{{ ev.time }}</span>
						<button
							type="button"
							class="flex h-6 w-6 items-center justify-center rounded-pill text-ink-subtle opacity-0 group-hover:opacity-100 hover:bg-surface-hover hover:text-brand"
							@click="$emit('edit', ev)"
						>
							<DRIcon name="pencil" :size="11" stroke="currentColor" />
						</button>
					</div>
				</div>
			</div>
		</div>
	</Teleport>
</template>

<script setup lang="ts">
	import { computed } from 'vue';
	import DRIcon from './DRIcon.vue';
	import type { RoutineEntry } from '@/types/dailyRoutine';

	const props = defineProps<{
		open: boolean;
		anchor: { x: number; y: number } | null;
		events: RoutineEntry[];
	}>();

	defineEmits<{
		(e: 'close'): void;
		(e: 'toggle', ev: RoutineEntry): void;
		(e: 'edit', ev: RoutineEntry): void;
	}>();

	const popoverStyle = computed(() => {
		const a = props.anchor;
		if (!a) return { top: '50%', left: '50%', transform: 'translate(-50%, -50%)' };
		return {
			top: `${Math.min(a.y, window.innerHeight - 320)}px`,
			left: `${Math.min(a.x, window.innerWidth - 360)}px`,
		};
	});
</script>
