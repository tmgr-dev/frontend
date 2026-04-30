<template>
	<div
		class="dr-week-chip absolute cursor-pointer overflow-hidden rounded-md border border-line"
		:class="allDone ? 'opacity-50' : ''"
		:style="containerStyle"
		@click.stop="onOpen"
	>
		<div class="flex h-1.5" :style="{ flexShrink: 0 }">
			<span v-for="(c, i) in stripes" :key="i" class="flex-1" :style="{ background: c }" />
		</div>
		<div v-if="mode === 'stack'" class="flex flex-col gap-0.5 p-1.5 text-2xs text-ink dark:text-white">
			<div class="flex items-center justify-between text-[10px] uppercase tracking-wider text-ink-subtle dark:text-white/70">
				<span>{{ events.length }} events</span>
				<span>{{ rangeLabel }}</span>
			</div>
			<div class="flex flex-col gap-0.5 overflow-hidden">
				<div
					v-for="ev in events.slice(0, fits)"
					:key="(ev.source.entry || ev.source).task_id"
					class="flex items-center gap-1 truncate"
					:title="(ev.source.entry || ev.source).title"
				>
					<span class="h-1 w-1 shrink-0 rounded-pill" :style="{ background: (ev.source.entry || ev.source).routine_category.color }" />
					<span class="text-[10px] tabular-nums text-ink-subtle dark:text-white/70">{{ (ev.source.entry || ev.source).time }}</span>
					<span class="truncate">{{ (ev.source.entry || ev.source).title }}</span>
				</div>
				<div v-if="hidden > 0" class="text-[10px] text-ink-subtle dark:text-white/55">+ {{ hidden }} more</div>
			</div>
		</div>
		<div v-else class="flex h-full items-center justify-center px-1 text-2xs font-semibold text-ink dark:text-white/80">
			+{{ events.length }}
		</div>
	</div>
	<ClusterPopover
		:open="open"
		:anchor="anchor"
		:events="rawEvents"
		@close="open = false"
		@toggle="onToggle"
		@edit="onEdit"
	/>
</template>

<script setup lang="ts">
	import { computed, ref } from 'vue';
	import ClusterPopover from './ClusterPopover.vue';
	import { fmtTime } from '@/utils/dailyRoutines/dateHelpers';
	import type { RoutineEntry } from '@/types/dailyRoutine';
	import type { PositionedEvent } from '@/utils/dailyRoutines/lanePacking';

	const HOUR_PX = 56;
	const ROW_H = 14;
	const HEADER_H = 18;

	const props = defineProps<{
		events: PositionedEvent<{ source: RoutineEntry }>[];
		startMin: number;
		endMin: number;
		mode: 'stack' | 'pill';
	}>();

	const emit = defineEmits<{
		(e: 'toggle', ev: RoutineEntry): void;
		(e: 'edit', ev: RoutineEntry): void;
	}>();

	const open = ref(false);
	const anchor = ref<{ x: number; y: number } | null>(null);

	const top = computed(() => (props.startMin / 60) * HOUR_PX);
	const height = computed(() => Math.max(24, ((props.endMin - props.startMin) / 60) * HOUR_PX - 2));
	const stripes = computed(() => {
		const seen = new Set<string>();
		const out: string[] = [];
		for (const e of props.events) {
			const entry = (e.source as any).entry || e.source;
			const c = entry?.routine_category?.color || '#888';
			if (!seen.has(c)) {
				seen.add(c);
				out.push(c);
			}
		}
		return out;
	});
	const fits = computed(() => Math.max(1, Math.floor((height.value - 6 - HEADER_H) / ROW_H)));
	const hidden = computed(() => Math.max(0, props.events.length - fits.value));
	const rangeLabel = computed(() =>
		`${fmtTime(Math.floor(props.startMin / 60), props.startMin % 60)}–${fmtTime(Math.floor(props.endMin / 60) % 24, props.endMin % 60)}`,
	);
	const allDone = computed(() => props.events.every(e => {
		const entry = (e.source as any).entry || e.source;
		return entry?.completed;
	}));
	const rawEvents = computed(() => props.events.map(e => ((e.source as any).entry || e.source) as RoutineEntry));

	const containerStyle = computed(() => ({
		top: `${top.value}px`,
		height: `${height.value}px`,
		left: props.mode === 'stack' ? '4px' : '3px',
		right: props.mode === 'stack' ? '6px' : '3px',
		background: 'rgba(127,127,127,.08)',
	}));

	function onOpen(e: MouseEvent) {
		anchor.value = { x: e.clientX + 8, y: e.clientY + 8 };
		open.value = true;
	}
	function onToggle(ev: RoutineEntry) {
		emit('toggle', ev);
	}
	function onEdit(ev: RoutineEntry) {
		emit('edit', ev);
		open.value = false;
	}
</script>
