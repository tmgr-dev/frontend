<template>
	<div
		v-if="isDemo"
		role="status"
		data-tour="ghost.panel"
		data-testid="ghost-panel"
		class="fixed bottom-4 left-1/2 z-40 flex max-w-[calc(100vw-2rem)] -translate-x-1/2 items-center gap-3 rounded-full border border-violet-300 bg-violet-100 py-1.5 pr-1.5 pl-4 text-sm text-violet-900 shadow-lg dark:border-violet-500/30 dark:bg-violet-950 dark:text-violet-200"
	>
		<template v-if="playing">
			<span class="min-w-0"
				>Simulated locally. No real agent is running. Press Esc to stop.</span
			>
			<button
				type="button"
				data-testid="ghost-skip"
				class="shrink-0 rounded-full px-3 py-1 font-medium hover:bg-violet-200 dark:hover:bg-violet-500/20"
				@click="skip"
			>
				Skip
			</button>
		</template>
		<template v-else>
			<span class="min-w-0">Demo agent</span>
			<button
				type="button"
				data-testid="ghost-replay"
				class="shrink-0 rounded-full px-3 py-1 font-medium hover:bg-violet-200 dark:hover:bg-violet-500/20"
				@click="replay"
			>
				Replay
			</button>
		</template>
	</div>
</template>

<script lang="ts">
	import { getTask } from '@/actions/tmgr/tasks';
	import { useDemoWorkspace } from '@/composable/useDemoWorkspace';
	import { deliverToWorkspace } from '@/composable/usePusher';
	import { recoverDemo, resetDemoCard } from '@/local/demoSeed';
	import {
		activeLocalWorkspace,
		listLocalWorkspaces,
		localContext,
	} from '@/local/runtime';
	import type { LocalWorkspace } from '@/local/types';
	import {
		buildScenario,
		createAxiosGhostEnv,
		createGhostPlayer,
		onGhostPlayRequested,
		registerGhostStop,
	} from '@/onboarding/ghost';
	import $axios from '@/plugins/axios';
	import { onEscape, prefersReducedMotion } from '@/tours/a11y';
	import { isInputBusy } from '@/tours/boot';
	import { tourBus } from '@/tours/events';
	import { useTours } from '@/tours/useTours';
	import { requestCache } from '@/utils/requestCache';
	import { defineComponent, onBeforeUnmount, onMounted, ref, watch } from 'vue';
	import { useStore } from 'vuex';

	const PRE_ROLL_MS = 900;
	const TOUR_DIALOG = '[role="dialog"][aria-labelledby^="tour-"]';
	const NO_USER = { id: 0, name: '', email: '' };

	const wait = (ms: number) =>
		new Promise<void>((resolve) => setTimeout(resolve, ms));

	export default defineComponent({
		name: 'GhostRunHost',
		setup() {
			const store = useStore();
			const tours = useTours();
			const { isDemo, code } = useDemoWorkspace();
			const playing = ref(false);
			let player: ReturnType<typeof createGhostPlayer> | null = null;
			let playingCode: string | null = null;
			let skipRequested = false;
			let disposed = false;
			let stopping = false;
			let current: Promise<void> | null = null;

			const refreshTask = async (workspaceId: number, taskId: number) => {
				requestCache.invalidate(`task-${taskId}`);
				requestCache.invalidate(/^tasks-status-/);
				if (activeLocalWorkspace()?.id !== workspaceId) return;
				const task = await getTask(taskId);
				if (task)
					deliverToWorkspace(workspaceId, (h) =>
						h.onTaskUpdated?.(task, 'updated'),
					);
			};

			const reloadTimers = () => store.commit('incrementReloadActiveTasksKey');

			const recover = async (workspace: LocalWorkspace) => {
				const known = await listLocalWorkspaces();
				if (!known.some((w) => w.code === workspace.code)) return;
				const ctx = await localContext(workspace, NO_USER);
				const touched = await recoverDemo(ctx);
				if (!touched.length) return;
				reloadTimers();
				await Promise.all(
					touched.map((id) => refreshTask(workspace.id, id).catch(() => {})),
				);
			};

			const recoverAll = async () => {
				const list = await listLocalWorkspaces();
				for (const workspace of list.filter((w) => w.demo))
					await recover(workspace).catch((error) =>
						console.error('[ghost] recover failed', error),
					);
			};

			let ready: Promise<void> = Promise.resolve();

			const cancelled = (workspace: LocalWorkspace) =>
				disposed || stopping || activeLocalWorkspace()?.code !== workspace.code;

			const run = async () => {
				if (playing.value) return;
				const workspace = activeLocalWorkspace();
				if (!workspace?.demo) return;
				playing.value = true;
				playingCode = workspace.code;
				skipRequested = false;
				try {
					await ready;
					await wait(PRE_ROLL_MS);
					if (cancelled(workspace)) return;
					const ctx = await localContext(workspace, NO_USER);
					const card = await resetDemoCard(ctx);
					if (!card) return;
					await refreshTask(workspace.id, card.taskId);
					if (cancelled(workspace)) {
						await recover(workspace);
						return;
					}
					const env = createAxiosGhostEnv($axios, {
						workspaceId: workspace.id,
						taskChanged: (taskId) =>
							void refreshTask(workspace.id, taskId).catch(() => {}),
						runChanged: (run) =>
							deliverToWorkspace(workspace.id, (h) =>
								h.onAgentWorkChanged?.(run),
							),
						timerChanged: reloadTimers,
					});
					player = createGhostPlayer({
						steps: buildScenario(card),
						env,
						onError: (error) => console.error('[ghost] step failed', error),
					});
					const running = player.start();
					if (skipRequested) player.skip();
					const outcome = await running;
					if (outcome === 'aborted' || outcome === 'failed')
						await recover(workspace);
					else tourBus.emit('ghost.finished');
				} catch (error) {
					console.error('[ghost] playback failed', error);
					await recover(workspace).catch(() => {});
				} finally {
					player = null;
					playingCode = null;
					playing.value = false;
				}
			};

			const play = () => {
				if (!current)
					current = run().finally(() => {
						current = null;
					});
				return current;
			};

			const stop = async () => {
				stopping = true;
				try {
					player?.abort();
					await current;
				} finally {
					stopping = false;
				}
			};

			const skip = () => {
				if (player) player.skip();
				else skipRequested = true;
			};

			const replay = () => void play();

			const offRequest = onGhostPlayRequested(() => {
				if (prefersReducedMotion()) tourBus.emit('ghost.finished');
				else void play();
			});

			const escapeAllowed = () =>
				playing.value && !isInputBusy(document, TOUR_DIALOG);

			const offEscape = onEscape(
				() => {
					if (escapeAllowed()) skip();
				},
				{
					swallow: escapeAllowed,
					ownDialog: () => document.querySelector(TOUR_DIALOG),
				},
			);

			watch(code, (next) => {
				if (playing.value && next !== playingCode) player?.abort();
			});

			watch(
				() => {
					const active = tours.active.value;
					return active?.tour.id === 'desktop.ghost-watch' &&
						active.step.id === 's2'
						? active.tour.id
						: null;
				},
				async (started) => {
					if (!started) return;
					const { watchDemo } = await import('@/local/demoApi');
					await watchDemo();
				},
			);

			onMounted(() => {
				ready = recoverAll();
				unregisterStop = registerGhostStop(stop);
			});

			let unregisterStop = () => {};

			onBeforeUnmount(() => {
				disposed = true;
				unregisterStop();
				offRequest();
				offEscape();
				player?.abort();
			});

			return { isDemo, playing, skip, replay };
		},
	});
</script>
