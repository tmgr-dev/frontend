<template>
	<span v-if="false" />
</template>

<script lang="ts">
	import { getTask } from '@/actions/tmgr/tasks';
	import { toast } from '@/components/ui/toast';
	import { createAhaWatcher, type AhaWatcher } from '@/onboarding/aha';
	import { tourBus } from '@/tours/events';
	import { onboardingApi } from '@/tours/api';
	import { useTours } from '@/tours/useTours';
	import { defineComponent, onBeforeUnmount, onMounted } from 'vue';
	import { useStore } from 'vuex';

	const safeStorage = () => ({
		getItem: (k: string) => {
			try {
				return localStorage.getItem(k);
			} catch {
				return null;
			}
		},
		setItem: (k: string, v: string) => {
			try {
				localStorage.setItem(k, v);
			} catch {}
		},
	});

	export default defineComponent({
		name: 'AhaWatcher',
		setup() {
			const store = useStore();
			const tours = useTours();
			let watcher: AhaWatcher | null = null;
			let stop: (() => void) | null = null;
			let off: (() => void) | null = null;

			onMounted(() => {
				watcher = createAhaWatcher({
					userId: () => store.state.user?.id ?? null,
					intent: () => tours.onboarding.value?.intent ?? null,
					intentAt: () => {
						const at = (tours.onboarding.value as { intent_at?: string } | null)
							?.intent_at;
						const ms = at ? Date.parse(at) : NaN;
						return Number.isFinite(ms) ? ms : null;
					},
					now: Date.now,
					storage: safeStorage(),
					fetchFacts: (keys, taskId) => onboardingApi().fetchFacts(keys, taskId),
					toast: (title) => toast({ title }),
					visibility: {
						hidden: () => document.visibilityState === 'hidden',
						onChange: (fn) => {
							document.addEventListener('visibilitychange', fn);
							return () => document.removeEventListener('visibilitychange', fn);
						},
					},
					setTimer: (fn, ms) => {
						const id = setTimeout(fn, ms);
						return () => clearTimeout(id);
					},
				});
				stop = watcher.start();
				off = tourBus.on('timer.stopped', async (payload) => {
					const taskId = payload.taskId;
					if (taskId == null) return;
					let title = 'your task';
					let minutes = 1;
					try {
						const task = await getTask(Number(taskId));
						title = task.title || title;
						minutes = Math.max(1, Math.floor((task.common_time || 0) / 60));
					} catch {}
					await watcher?.onTimerStopped({ taskId: taskId as number, minutes, title });
				});
			});

			onBeforeUnmount(() => {
				off?.();
				stop?.();
			});

			return {};
		},
	});
</script>
