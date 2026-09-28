<template>
	<footer
		class="statusbar-offset fixed bottom-0 right-0 z-30 flex h-[var(--statusbar-h)] items-center bg-sidebar text-xs text-muted-foreground"
	>
		<div class="flex h-full min-w-0 items-center gap-1.5 px-3">
			<span
				v-if="workspace?.is_local"
				class="shrink-0 rounded bg-amber-500/15 px-1.5 py-0.5 text-2xs font-semibold uppercase text-amber-600 dark:text-amber-400"
				:title="workspace.path"
				>Local</span
			>
			<router-link
				v-if="workspace"
				:to="`/${workspace.code}/list`"
				class="truncate hover:text-foreground"
			>
				{{ workspace.name }}
			</router-link>
			<ChevronRight v-if="workspace && title" class="h-3 w-3 shrink-0" />
			<span class="truncate text-foreground">{{ title }}</span>
		</div>

		<button
			v-if="runningTask"
			type="button"
			class="flex h-full min-w-0 items-center gap-2 rounded-md px-2 text-foreground hover:bg-muted"
			:title="runningTask.title"
			@click="openTask(runningTask)"
		>
			<span
				class="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-emerald-500"
			/>
			<span class="max-w-[260px] truncate">{{ runningTask.title }}</span>
			<span class="font-mono font-semibold text-emerald-500">
				{{ runningElapsed }}
			</span>
			<span v-if="tasks.length > 1" class="text-muted-foreground">
				+{{ tasks.length - 1 }}
			</span>
		</button>

		<div class="flex-1" />

		<component
			:is="item.command ? 'button' : 'div'"
			v-for="item in pluginItems"
			:key="`${item.pluginId}:${item.itemId}`"
			:type="item.command ? 'button' : undefined"
			:title="`${item.pluginName} (plugin)${item.tooltip ? `: ${item.tooltip}` : ''}`"
			:class="[
				'flex h-full items-center px-3',
				item.command && 'hover:bg-muted hover:text-foreground',
			]"
			@click="item.command && runPluginCommand(item)"
		>
			{{ item.text }}
		</component>

		<DropdownMenu>
			<DropdownMenuTrigger as-child>
				<button
					type="button"
					class="flex h-full items-center px-2 text-muted-foreground hover:bg-muted hover:text-foreground"
					:class="dndActive && 'text-amber-600 dark:text-amber-400'"
					:title="dndActive ? 'Do not disturb is on' : 'Do not disturb'"
				>
					<BellOff v-if="dndActive" class="h-3.5 w-3.5" />
					<Bell v-else class="h-3.5 w-3.5" />
				</button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end">
				<DropdownMenuItem
					v-for="option in dndOptions"
					:key="option.value"
					@select="setDnd(option.value)"
				>
					<Check
						class="mr-2 h-3.5 w-3.5"
						:class="dndOption === option.value ? 'opacity-100' : 'opacity-0'"
					/>
					{{ option.label }}
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>

		<div
			class="flex h-full items-center gap-1.5 px-3"
			:title="`Realtime: ${connectionState}`"
		>
			<span :class="['h-1.5 w-1.5 rounded-full', liveDotClass]" />
			{{ liveLabel }}
		</div>

		<button
			v-if="update.status !== 'idle'"
			type="button"
			class="flex h-full items-center gap-1.5 px-3 font-medium text-primary hover:bg-muted"
			:disabled="update.status === 'installing' || update.status === 'checking'"
			title="Restart to install the update"
			@click="installUpdate"
		>
			<Download class="h-3.5 w-3.5" />
			{{
				update.status === 'installing'
					? 'Updating…'
					: update.status === 'checking'
					? 'Checking for updates…'
					: `Update to v${update.version}`
			}}
		</button>

		<div v-if="version" class="flex h-full items-center px-3">
			v{{ version }}
		</div>

		<div class="flex h-full items-center gap-0.5 px-1.5">
			<button
				type="button"
				title="Ask AI"
				:class="[
					'flex h-6 w-7 items-center justify-center rounded-md hover:bg-muted hover:text-foreground',
					aiPanelOpen && 'bg-muted text-foreground',
				]"
				@click="toggleAi"
			>
				<Sparkles class="h-3.5 w-3.5" />
			</button>
			<ActiveCursorAgents />
		</div>
	</footer>
</template>

<script>
	import ActiveCursorAgents from '@/components/cursor/ActiveCursorAgents.vue';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuItem,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';
	import { usePusher } from '@/composable/usePusher';
	import { pluginHost, pluginState } from '@/pluginSystem/state';
	import store from '@/store';
	import { dndClock, dndState, isDndActive, setDnd } from '@/utils/dnd';
	import { installUpdate, updateState } from '@/utils/desktopUpdater';
	import {
		Bell,
		BellOff,
		Check,
		ChevronRight,
		Download,
		Sparkles,
	} from 'lucide-vue-next';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
	} from 'vue';

	const formatElapsed = (seconds) => {
		const h = Math.floor(seconds / 3600);
		const m = Math.floor((seconds % 3600) / 60);
		const s = seconds % 60;
		return [h, m, s].map((n) => String(n).padStart(2, '0')).join(':');
	};

	const DND_OPTIONS = [
		{ value: 'off', label: 'Off' },
		{ value: '1h', label: 'For 1 hour' },
		{ value: '3h', label: 'For 3 hours' },
		{ value: 'tomorrow', label: 'Until tomorrow' },
	];

	export default defineComponent({
		name: 'StatusBar',
		components: {
			ActiveCursorAgents,
			DropdownMenu,
			DropdownMenuContent,
			DropdownMenuItem,
			DropdownMenuTrigger,
			Bell,
			BellOff,
			Check,
			ChevronRight,
			Download,
			Sparkles,
		},
		props: {
			tasks: { type: Array, default: () => [] },
		},
		setup(props) {
			const { connectionState } = usePusher();
			const now = ref(Math.floor(Date.now() / 1000));
			const version = ref('');
			let ticker = null;

			onMounted(async () => {
				ticker = setInterval(() => {
					now.value = Math.floor(Date.now() / 1000);
				}, 1000);
				try {
					const { getVersion } = await import('@tauri-apps/api/app');
					version.value = await getVersion();
				} catch {
					version.value = '';
				}
			});
			onBeforeUnmount(() => clearInterval(ticker));

			const runningTask = computed(() => props.tasks[0] || null);
			const runningElapsed = computed(() => {
				const task = runningTask.value;
				if (!task) return '';
				const started = task.start_time > 0 ? now.value - task.start_time : 0;
				return formatElapsed(Math.max(0, (task.common_time || 0) + started));
			});

			const liveLabel = computed(
				() =>
					({
						connected: 'Live',
						connecting: 'Connecting…',
						reconnecting: 'Reconnecting…',
						error: 'Offline',
					}[connectionState.value] || 'Offline'),
			);
			const liveDotClass = computed(() =>
				connectionState.value === 'connected'
					? 'bg-emerald-500'
					: ['connecting', 'reconnecting'].includes(connectionState.value)
					? 'bg-amber-500'
					: 'bg-muted-foreground',
			);

			return {
				workspace: computed(() => store.getters.currentWorkspace),
				title: computed(() => store.state.metaTitle || ''),
				aiPanelOpen: computed(() => store.state.aiPanelOpen),
				toggleAi: () => store.commit('toggleAiPanel'),
				openTask: (task) => {
					const workspaceId = task.workspace_id;
					if (workspaceId == null || workspaceId === store.getters.currentWorkspaceId) {
						store.commit('setCurrentTaskIdForModal', task.id);
					}
				},
				runningTask,
				runningElapsed,
				connectionState,
				liveLabel,
				liveDotClass,
				version,
				update: updateState,
				installUpdate,
				pluginItems: computed(() => Object.values(pluginState.statusBar)),
				runPluginCommand: (item) =>
					pluginHost()
						?.runCommand(item.pluginId, item.command)
						.catch(() => undefined),
				dndOptions: DND_OPTIONS,
				dndOption: computed(() => dndState.option),
				dndActive: computed(() => isDndActive(dndClock.now)),
				setDnd,
			};
		},
	});
</script>
