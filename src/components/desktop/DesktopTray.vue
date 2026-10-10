<template>
	<AlertDialog :open="!!away" @update:open="(open) => !open && (away = null)">
		<AlertDialogContent>
			<AlertDialogHeader>
				<AlertDialogTitle>
					You were away for {{ awayLabel }}
				</AlertDialogTitle>
				<AlertDialogDescription>
					Since {{ awaySinceLabel }} the timer kept running on
					{{ runningLabel }}.
				</AlertDialogDescription>
			</AlertDialogHeader>
			<AlertDialogFooter>
				<AlertDialogCancel>Keep the time</AlertDialogCancel>
				<AlertDialogAction @click="stopAll">Stop the timer</AlertDialogAction>
			</AlertDialogFooter>
		</AlertDialogContent>
	</AlertDialog>
</template>

<script>
	import {
		startTaskTimeCounter,
		stopTaskTimeCounter,
	} from '@/actions/tmgr/tasks';
	import {
		AlertDialog,
		AlertDialogAction,
		AlertDialogCancel,
		AlertDialogContent,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
	} from '@/components/ui/alert-dialog';
	import { usePusher } from '@/composable/usePusher';
	import { pluginState } from '@/pluginSystem/state';
	import router from '@/router';
	import store from '@/store';
	import { isDndActive, setDnd } from '@/utils/dnd';
	import { showSurface } from '@/utils/moduleSurfaces';
	import { openTaskPreferringWindow } from '@/utils/openTaskInWorkspace';
	import {
		buildTrayState,
		formatAway,
		loadRecent,
		pushTrayState,
		rememberRecent,
		saveRecent,
		sameTask,
	} from '@/utils/desktopTray';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
		watch,
	} from 'vue';

	const reloadActiveTasks = () => store.commit('incrementReloadActiveTasksKey');

	const notify = (title, body) => {
		if (
			'Notification' in window &&
			Notification.permission === 'granted' &&
			!document.hasFocus() &&
			!isDndActive()
		) {
			new Notification(title, { body });
		}
	};

	const showWindow = async () => {
		const { getCurrentWindow } = await import('@tauri-apps/api/window');
		const win = getCurrentWindow();
		await win.unminimize();
		await win.show();
		await win.setFocus();
	};

	export default defineComponent({
		name: 'DesktopTray',
		components: {
			AlertDialog,
			AlertDialogAction,
			AlertDialogCancel,
			AlertDialogContent,
			AlertDialogDescription,
			AlertDialogFooter,
			AlertDialogHeader,
			AlertDialogTitle,
		},
		props: {
			tasks: { type: Array, default: () => [] },
		},
		setup(props) {
			const pusher = usePusher();
			const isOn = (key) => store.getters['featureToggles/isFeatureEnabled'](key);
			const recent = ref([]);
			const away = ref(null);
			const unlisteners = [];
			let userSubscription = null;
			let subscribedUserId = null;

			const pluginTraySections = () =>
				Object.values(pluginState.trayItems).map((entry) => ({
					pluginName: entry.pluginName,
					title: entry.title,
					items: entry.items,
				}));

			const sync = () => {
				const userId = store.state.user?.id;
				if (!userId) return;
				recent.value = rememberRecent(recent.value, props.tasks);
				saveRecent(userId, recent.value);
				pushTrayState({
					...buildTrayState(
						props.tasks,
						showSurface('timer.tray-recent', isOn) ? recent.value : [],
					),
					pluginSections: pluginTraySections(),
					trayTitle: pluginState.trayTitle,
				});
			};

			watch(() => props.tasks, sync, { deep: true });
			watch(() => showSurface('timer.tray-recent', isOn), sync);
			watch(() => [pluginState.trayItems, pluginState.trayTitle], sync, {
				deep: true,
			});

			const stopAll = async () => {
				await Promise.all(
					props.tasks.map((task) =>
						stopTaskTimeCounter(task.id, task.workspace_id),
					),
				);
			};

			const stopOne = ({ taskId, workspaceId }) =>
				stopTaskTimeCounter(taskId, workspaceId);

			const switchTo = async (target) => {
				const chosen = { id: target.taskId, workspaceId: target.workspaceId };
				await Promise.all(
					props.tasks
						.filter((task) => !sameTask(task, chosen))
						.map((task) => stopTaskTimeCounter(task.id, task.workspace_id)),
				);
				if (!props.tasks.some((task) => sameTask(task, chosen))) {
					await startTaskTimeCounter(target.taskId, target.workspaceId);
				}
			};

			const safely = (action) => async (event) => {
				try {
					await action(event.payload);
				} catch (error) {
					console.error('tray action failed', error);
					reloadActiveTasks();
				}
			};

			const subscribe = (userId) => {
				if (userSubscription && subscribedUserId) {
					pusher.unsubscribeHandler(
						`App.User.${subscribedUserId}`,
						userSubscription,
					);
				}
				userSubscription = null;
				subscribedUserId = userId || null;
				if (!userId) return;
				userSubscription = pusher.subscribeToUser(userId, {
					onNotificationCreated: (data) => {
						const n = data?.notification;
						if (n) notify(n.title || 'TMGR', n.message || '');
					},
				});
			};

			watch(
				() => store.state.user?.id,
				(userId) => {
					recent.value = userId ? loadRecent(userId) : [];
					subscribe(userId);
					sync();
				},
				{ immediate: true },
			);

			onMounted(async () => {
				const { listen } = await import('@tauri-apps/api/event');
				unlisteners.push(
					await listen('tray://open', ({ payload }) =>
						openTaskPreferringWindow(payload, store, router, showWindow),
					),
					await listen('tray://shortcuts', () =>
						router.push('/settings?tab=desktop'),
					),
					await listen('tray://stop', safely(stopOne)),
					await listen('tray://switch', safely(switchTo)),
					await listen('tray://dnd', ({ payload }) => setDnd(payload)),
					await listen('idle://returned', async (event) => {
						reloadActiveTasks();
						if (!props.tasks.length) return;
						away.value = event.payload;
						notify(
							`Away for ${formatAway(event.payload.awaySeconds)}`,
							'The timer kept running. Open TMGR to keep or stop it.',
						);
						await showWindow();
					}),
				);
				if ('Notification' in window && Notification.permission !== 'granted') {
					Notification.requestPermission();
				}
			});

			onBeforeUnmount(() => {
				unlisteners.forEach((unlisten) => unlisten());
				subscribe(null);
				pushTrayState({ running: [], recent: [] });
			});

			const runningLabel = computed(() =>
				props.tasks.map((task) => task.title).join(', '),
			);

			return {
				away,
				awayLabel: computed(() =>
					away.value ? formatAway(away.value.awaySeconds) : '',
				),
				awaySinceLabel: computed(() =>
					away.value
						? new Date(away.value.awaySince * 1000).toLocaleTimeString([], {
								hour: '2-digit',
								minute: '2-digit',
						  })
						: '',
				),
				runningLabel,
				stopAll,
			};
		},
	});
</script>
