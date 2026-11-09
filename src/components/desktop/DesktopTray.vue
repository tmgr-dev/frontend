<template>
	<AlertDialog :open="!!away" @update:open="(open) => !open && (away = null)">
		<AlertDialogContent>
			<AlertDialogHeader>
				<AlertDialogTitle>{{
					$t('desktop.tray.awayTitle', { duration: awayLabel })
				}}</AlertDialogTitle>
				<AlertDialogDescription>
					{{
						$t('desktop.tray.awayDescription', {
							since: awaySinceLabel,
							tasks: runningLabel,
						})
					}}
				</AlertDialogDescription>
			</AlertDialogHeader>
			<AlertDialogFooter>
				<AlertDialogCancel>{{ $t('desktop.tray.keepTime') }}</AlertDialogCancel>
				<AlertDialogAction @click="stopAll">{{
					$t('desktop.tray.stopTimer')
				}}</AlertDialogAction>
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
	import { ToastAction, useToast } from '@/components/ui/toast';
	import { currentLocale, t } from '@/i18n';
	import { resolveTaskByKey } from '@/composable/useTaskKeyLinks';
	import { usePusher } from '@/composable/usePusher';
	import { pluginState } from '@/pluginSystem/state';
	import router from '@/router';
	import store from '@/store';
	import {
		buildTrayState,
		formatAway,
		loadRecent,
		pushTrayState,
		rememberRecent,
		sameTask,
		saveRecent,
		toTrayTask,
	} from '@/utils/desktopTray';
	import {
		registerActivityContext,
		stopActivity,
		syncActivity,
	} from '@/utils/activityHints';
	import { isDndActive, setDnd } from '@/utils/dnd';
	import {
		decideGitTrack,
		loadGitTrackMode,
		syncGitTrack,
		unwatchGitTrack,
	} from '@/utils/gitTrack';
	import { showSurface } from '@/utils/moduleSurfaces';
	import { openTaskPreferringWindow } from '@/utils/openTaskInWorkspace';
	import {
		computed,
		defineComponent,
		h,
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
			timerRunning: { type: Boolean, default: true },
		},
		setup(props) {
			const pusher = usePusher();
			const { toast } = useToast();
			const isOn = (key) =>
				store.getters['featureToggles/isFeatureEnabled'](key);
			const recent = ref([]);
			const away = ref(null);
			const unlisteners = [];
			let unmounted = false;
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

			const onGitBranch = async ({ key }) => {
				if (!isOn('task.countdown')) return;
				const workspaceBefore = store.getters.currentWorkspaceId;
				const found = await resolveTaskByKey(key);
				if (!found || store.getters.currentWorkspaceId !== workspaceBefore) {
					return;
				}
				const candidate = {
					...toTrayTask(found),
					workspaceId: found.workspace_id ?? workspaceBefore,
					statusType: store.getters.workspaceStatusById(found.status_id)?.type,
				};
				const decision = decideGitTrack({
					mode: loadGitTrackMode(),
					task: candidate,
					runningTasks: props.tasks,
				});
				if (decision === 'ignore') return;
				const target = {
					taskId: found.id,
					workspaceId: candidate.workspaceId,
				};
				if (decision === 'switch') {
					await switchTo(target);
					await tasksRefreshed();
					toast({
						description: t('desktop.tray.timerSwitched', { key }),
					});
					return;
				}
				const message = props.tasks.length
					? t('desktop.tray.switchTimerTo', { key })
					: t('desktop.tray.startTimerOn', { key });
				toast({
					title: message,
					duration: 30000,
					action: h(
						ToastAction,
						{
							altText: t('desktop.tray.startTimer'),
							onClick: () => switchTo(target).catch(() => reloadActiveTasks()),
						},
						() => t('desktop.tray.startTimer'),
					),
				});
				notify(message, t('desktop.tray.branchMatchBody'));
			};

			let gitQueue = Promise.resolve();
			let tasksChanged = null;
			const tasksRefreshed = () =>
				new Promise((resolve) => {
					const timer = setTimeout(resolve, 3000);
					tasksChanged = () => {
						clearTimeout(timer);
						resolve();
					};
				});
			watch(
				() => props.tasks,
				() => tasksChanged?.(),
				{ deep: true },
			);

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

			const activityContext = () => ({
				userId: store.state.user?.id,
				workspaceId: store.getters.currentWorkspaceId,
				timerRunning: props.timerRunning,
				modulesKnown: store.getters['featureToggles/modulesKnown'],
				sessionTimeOn:
					store.getters['featureToggles/moduleEntry']('session_time')
						?.enabled === true,
			});
			registerActivityContext(activityContext);
			const pushActivity = () =>
				syncActivity().catch((error) =>
					console.error('activity sync failed', error),
				);
			watch(() => JSON.stringify(activityContext()), pushActivity);

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
					await listen('git-track://branch', (event) => {
						gitQueue = gitQueue.then(() => safely(onGitBranch)(event));
					}),
					await listen('tray://dnd', ({ payload }) => setDnd(payload)),
					await listen('idle://returned', async (event) => {
						reloadActiveTasks();
						if (!props.tasks.length) return;
						away.value = event.payload;
						notify(
							t('desktop.tray.awayNotifyTitle', {
								duration: formatAway(event.payload.awaySeconds),
							}),
							t('desktop.tray.awayNotifyBody'),
						);
						await showWindow();
					}),
				);
				if ('Notification' in window && Notification.permission !== 'granted') {
					Notification.requestPermission();
				}
				if (unmounted) return;
				syncGitTrack().catch((error) =>
					console.error('git track sync failed', error),
				);
				pushActivity();
			});

			onBeforeUnmount(() => {
				unmounted = true;
				unlisteners.forEach((unlisten) => unlisten());
				subscribe(null);
				pushTrayState({ running: [], recent: [] });
				unwatchGitTrack().catch(() => {});
				registerActivityContext(null);
				stopActivity().catch(() => {});
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
						? new Date(away.value.awaySince * 1000).toLocaleTimeString(
								currentLocale(),
								{
									hour: '2-digit',
									minute: '2-digit',
								},
							)
						: '',
				),
				runningLabel,
				stopAll,
			};
		},
	});
</script>
