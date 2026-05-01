<template>
	<Teleport to="body">
		<div
			class="fixed inset-0 z-[10001]"
			@click="$emit('close')"
			@contextmenu.prevent="$emit('close')"
		>
			<div
				ref="menuRef"
				class="absolute min-w-[180px] overflow-hidden rounded-card border border-line bg-surface py-1 shadow-tmgr-lg"
				:style="positionStyle"
				@click.stop
				@contextmenu.prevent
			>
				<button
					type="button"
					class="flex w-full items-center gap-2.5 px-3 py-2 text-left text-sm text-ink transition-colors hover:bg-surface-hover"
					@click="onEdit"
				>
					<DRIcon name="pencil" :size="13" stroke="currentColor" />
					Edit
				</button>
				<button
					type="button"
					class="flex w-full items-center gap-2.5 px-3 py-2 text-left text-sm text-ink transition-colors hover:bg-surface-hover"
					@click="onArchive"
				>
					<DRIcon name="archive" :size="13" stroke="currentColor" />
					Archive
				</button>
				<div class="my-1 h-px bg-line" />
				<button
					type="button"
					class="flex w-full items-center gap-2.5 px-3 py-2 text-left text-sm text-status-fix-fg transition-colors hover:bg-status-fix-bg/40"
					@click="onDelete"
				>
					<DRIcon name="trash" :size="13" stroke="currentColor" />
					Delete
				</button>
			</div>
		</div>
	</Teleport>
</template>

<script setup lang="ts">
	import { computed, onMounted, onBeforeUnmount, ref, nextTick } from 'vue';
	import DRIcon from './DRIcon.vue';
	import type { RoutineEntry } from '@/types/dailyRoutine';

	const props = defineProps<{
		entry: RoutineEntry;
		x: number;
		y: number;
	}>();

	const emit = defineEmits<{
		(e: 'close'): void;
		(e: 'edit', entry: RoutineEntry): void;
		(e: 'archive', entry: RoutineEntry): void;
		(e: 'delete', entry: RoutineEntry): void;
	}>();

	const menuRef = ref<HTMLElement | null>(null);
	const adjustedX = ref(props.x);
	const adjustedY = ref(props.y);

	const positionStyle = computed(() => ({
		left: `${adjustedX.value}px`,
		top: `${adjustedY.value}px`,
	}));

	function clampPosition() {
		const el = menuRef.value;
		if (!el) return;
		const rect = el.getBoundingClientRect();
		const margin = 8;
		let x = props.x;
		let y = props.y;
		if (x + rect.width + margin > window.innerWidth) {
			x = window.innerWidth - rect.width - margin;
		}
		if (y + rect.height + margin > window.innerHeight) {
			y = window.innerHeight - rect.height - margin;
		}
		adjustedX.value = Math.max(margin, x);
		adjustedY.value = Math.max(margin, y);
	}

	function onKey(e: KeyboardEvent) {
		if (e.key === 'Escape') emit('close');
	}

	function onEdit() {
		emit('edit', props.entry);
		emit('close');
	}

	function onArchive() {
		emit('archive', props.entry);
		emit('close');
	}

	function onDelete() {
		// eslint-disable-next-line no-alert
		if (!confirm(`Delete "${props.entry.title}"?`)) return;
		emit('delete', props.entry);
		emit('close');
	}

	onMounted(async () => {
		document.addEventListener('keydown', onKey);
		await nextTick();
		clampPosition();
	});
	onBeforeUnmount(() => {
		document.removeEventListener('keydown', onKey);
	});
</script>
