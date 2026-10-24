<template>
	<div class="relative">
		<DropdownMenu @update:open="handleDropdownOpen">
			<DropdownMenuTrigger as-child>
				<button
					class="relative rounded-md p-2 text-current transition-colors hover:bg-surface-hover"
					:aria-label="`Active agents: ${activeAgents.length}`"
				>
					<Bot :size="20" />
					<span
						v-if="activeAgents.length > 0"
						class="absolute top-0.5 right-0.5 flex h-[18px] min-w-[18px] items-center justify-center rounded-[9px] bg-brand px-1 text-[10px] leading-none font-semibold text-on-accent"
					>
						{{ activeAgents.length > 99 ? '99+' : activeAgents.length }}
					</span>
				</button>
			</DropdownMenuTrigger>

			<DropdownMenuContent
				class="flex max-h-[520px] w-[340px] flex-col overflow-hidden p-0"
				align="end"
				:side-offset="8"
			>
				<div class="flex items-center justify-between px-4 py-3.5">
					<h3 class="m-0 text-base font-semibold text-ink">Active agents</h3>
					<button
						class="inline-flex items-center justify-center rounded p-1 text-current opacity-70 transition-opacity enabled:hover:opacity-100 disabled:cursor-not-allowed"
						:disabled="loading"
						aria-label="Refresh"
						@click.stop="loadAgents"
					>
						<LoaderCircle :size="14" :class="{ 'animate-spin': loading }" />
					</button>
				</div>

				<DropdownMenuSeparator />

				<div
					v-if="loading && activeAgents.length === 0"
					class="flex flex-col items-center justify-center gap-3 p-8 text-sm text-ink-muted"
				>
					<LoaderCircle :size="28" class="animate-spin opacity-50" />
					<p>Loading…</p>
				</div>

				<div
					v-else-if="activeAgents.length === 0"
					class="flex flex-col items-center justify-center gap-3 p-8 text-sm text-ink-muted"
				>
					<Bot :size="40" class="opacity-30" />
					<p>No active agents</p>
				</div>

				<div v-else class="max-h-[420px] overflow-y-auto p-1">
					<DropdownMenuItem
						v-for="agent in activeAgents"
						:key="agent.id"
						class="flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-2"
						:disabled="!canJump(agent)"
						@select="handleAgentClick(agent)"
					>
						<span
							class="h-2 w-2 shrink-0 rounded-full"
							:class="
								agent.status === 'RUNNING'
									? 'bg-status-done shadow-[0_0_6px_color-mix(in_srgb,var(--s-done)_60%,transparent)]'
									: 'bg-status-progress'
							"
							:title="agent.status"
						></span>
						<span class="flex min-w-0 flex-1 flex-col">
							<span class="truncate text-sm font-medium">{{
								labelFor(agent)
							}}</span>
							<span class="text-xs text-ink-subtle">
								Task #{{ agent.task_id }} · {{ statusLabel(agent.status) }}
							</span>
						</span>
						<ArrowUpRight
							v-if="canJump(agent)"
							:size="14"
							class="shrink-0 opacity-60"
						/>
					</DropdownMenuItem>
				</div>
			</DropdownMenuContent>
		</DropdownMenu>
	</div>
</template>

<script>
	import { getActiveCursorAgents } from '@/actions/tmgr/cursor';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuItem,
		DropdownMenuSeparator,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';
	import {
		canJumpToCursorAgentTask,
		cursorAgentLabel,
		cursorAgentTaskRoute,
		filterActiveCursorAgents,
	} from '@/utils/cursorAgents';
	import { createPollingLoop } from '@/utils/pollingLoop';
	import { ArrowUpRight, Bot, LoaderCircle } from 'lucide-vue-next';
	import { computed, defineComponent, onMounted, onUnmounted, ref } from 'vue';
	import { useRouter } from 'vue-router';

	const POLL_INTERVAL_MS = 20000;

	export default defineComponent({
		name: 'ActiveCursorAgents',
		components: {
			Bot,
			LoaderCircle,
			ArrowUpRight,
			DropdownMenu,
			DropdownMenuContent,
			DropdownMenuTrigger,
			DropdownMenuSeparator,
			DropdownMenuItem,
		},
		setup() {
			const router = useRouter();

			const agents = ref([]);
			const loading = ref(false);
			let disposed = false;

			const activeAgents = computed(() =>
				filterActiveCursorAgents(agents.value),
			);

			const loadAgents = async () => {
				loading.value = true;
				try {
					const result = await getActiveCursorAgents();
					if (!disposed) agents.value = result;
				} catch (error) {
					// Keep the last successful result on a transient failure.
				} finally {
					loading.value = false;
				}
			};

			const poller = createPollingLoop(loadAgents, POLL_INTERVAL_MS);

			const handleAgentClick = (agent) => {
				const target = cursorAgentTaskRoute(agent);
				if (target) {
					router.push(target);
				}
			};

			const handleDropdownOpen = (isOpen) => {
				if (isOpen) {
					void poller.run();
				}
			};

			onMounted(() => poller.start());
			onUnmounted(() => {
				disposed = true;
				poller.stop();
			});

			const statusLabel = (status) =>
				status === 'RUNNING' ? 'Running' : 'Starting';

			return {
				agents,
				loading,
				activeAgents,
				loadAgents: poller.run,
				handleAgentClick,
				handleDropdownOpen,
				labelFor: cursorAgentLabel,
				canJump: canJumpToCursorAgentTask,
				statusLabel,
			};
		},
	});
</script>
