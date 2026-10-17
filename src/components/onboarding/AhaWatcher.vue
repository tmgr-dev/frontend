<template>
	<span v-if="false" />
</template>

<script lang="ts">
	import { getTask } from '@/actions/tmgr/tasks';
	import { toast } from '@/components/ui/toast';
	import { createAhaWatcher, type AhaWatcher } from '@/onboarding/aha';
	import { onboardingApi } from '@/tours/api';
	import { tourBus } from '@/tours/events';
	import { useTours } from '@/tours/useTours';
	import { defineComponent, onBeforeUnmount, onMounted, watch } from 'vue';
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
			const firstSeenAt = (): number | null => {
				const uid = store.state.user?.id;
				if (uid == null) return null;
				const key = `tmgr:onboarding:aha-since:${uid}`;
				const storage = safeStorage();
				const saved = Number(storage.getItem(key));
				if (Number.isFinite(saved) && saved > 0) return saved;
				const now = Date.now();
				storage.setItem(key, String(now));
				return now;
			};
			let watcher: AhaWatcher | null = null;
			let stop: (() => void) | null = null;
			let off: (() => void) | null = null;
			let stopWatch: (() => void) | null = null;

			onMounted(() => {
				watcher = createAhaWatcher({
					userId: () => store.state.user?.id ?? null,
					intent: () => tours.onboarding.value?.intent ?? null,
					intentAt: firstSeenAt,
					inert: () => !!store.getters.isLocalOnly,
					now: Date.now,
					storage: safeStorage(),
					fetchFacts: (keys, taskId) =>
						onboardingApi().fetchFacts(keys, taskId),
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
				stopWatch = watch(
					() => [
						store.state.user?.id ?? null,
						tours.onboarding.value?.intent ?? null,
					],
					() => {
						stop?.();
						stop = watcher?.start() ?? null;
					},
					{ immediate: true },
				);
				off = tourBus.on('timer.stopped', async (payload) => {
					const taskId = payload.taskId;
					if (taskId == null || !watcher?.timerEligible()) return;
					let title = 'your task';
					let minutes = 1;
					try {
						const task = await getTask(Number(taskId));
						title = task.title || title;
						minutes = Math.max(1, Math.floor((task.common_time || 0) / 60));
					} catch {}
					await watcher?.onTimerStopped({
						taskId: taskId as number,
						minutes,
						title,
					});
				});
			});

			onBeforeUnmount(() => {
				off?.();
				stopWatch?.();
				stop?.();
			});

			return {};
		},
	});
</script>
