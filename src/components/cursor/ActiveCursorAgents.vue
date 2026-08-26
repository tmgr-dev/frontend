<template>
	<div class="active-agents">
		<DropdownMenu @update:open="handleDropdownOpen">
			<DropdownMenuTrigger as-child>
				<button
					class="active-agents-button relative"
					:aria-label="`Active agents: ${activeAgents.length}`"
				>
					<Bot :size="20" />
					<span v-if="activeAgents.length > 0" class="active-agents-badge">
						{{ activeAgents.length > 99 ? '99+' : activeAgents.length }}
					</span>
				</button>
			</DropdownMenuTrigger>

			<DropdownMenuContent
				class="active-agents-dropdown"
				align="end"
				:side-offset="8"
			>
				<div class="active-agents-header">
					<h3 class="active-agents-title">Active agents</h3>
					<button
						class="refresh-btn"
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
					class="active-agents-state"
				>
					<LoaderCircle :size="28" class="animate-spin opacity-50" />
					<p>Loading…</p>
				</div>

				<div v-else-if="activeAgents.length === 0" class="active-agents-state">
					<Bot :size="40" class="opacity-30" />
					<p>No active agents</p>
				</div>

				<div v-else class="active-agents-list">
					<DropdownMenuItem
						v-for="agent in activeAgents"
						:key="agent.id"
						class="active-agent-item"
						:disabled="!canJump(agent)"
						@select="handleAgentClick(agent)"
					>
						<span
							class="agent-status-dot"
							:class="agent.status === 'RUNNING' ? 'is-running' : 'is-creating'"
							:title="agent.status"
						></span>
						<span class="agent-info">
							<span class="agent-label">{{ labelFor(agent) }}</span>
							<span class="agent-meta">
								Task #{{ agent.task_id }} · {{ statusLabel(agent.status) }}
							</span>
						</span>
						<ArrowUpRight
							v-if="canJump(agent)"
							:size="14"
							class="agent-jump opacity-60"
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
			let pollTimer = null;

			const activeAgents = computed(() =>
				filterActiveCursorAgents(agents.value),
			);

			const loadAgents = async () => {
				loading.value = true;
				try {
					agents.value = await getActiveCursorAgents();
				} catch (error) {
					agents.value = [];
				} finally {
					loading.value = false;
				}
			};

			const handleAgentClick = (agent) => {
				const target = cursorAgentTaskRoute(agent);
				if (target) {
					router.push(target);
				}
			};

			const handleDropdownOpen = (isOpen) => {
				if (isOpen) {
					loadAgents();
				}
			};

			onMounted(() => {
				loadAgents();
				pollTimer = setInterval(loadAgents, POLL_INTERVAL_MS);
			});

			onUnmounted(() => {
				if (pollTimer) {
					clearInterval(pollTimer);
					pollTimer = null;
				}
			});

			const statusLabel = (status) =>
				status === 'RUNNING' ? 'Running' : 'Starting';

			return {
				agents,
				loading,
				activeAgents,
				loadAgents,
				handleAgentClick,
				handleDropdownOpen,
				labelFor: cursorAgentLabel,
				canJump: canJumpToCursorAgentTask,
				statusLabel,
			};
		},
	});
</script>

<style lang="scss" scoped>
	.active-agents {
		position: relative;
	}

	.active-agents-button {
		position: relative;
		padding: 0.5rem;
		border: none;
		background: transparent;
		color: currentColor;
		cursor: pointer;
		border-radius: 0.375rem;
		transition: background-color 0.2s;

		&:hover {
			background-color: rgba(0, 0, 0, 0.05);
		}

		.dark & {
			&:hover {
				background-color: rgba(255, 255, 255, 0.1);
			}
		}
	}

	.active-agents-badge {
		position: absolute;
		top: 2px;
		right: 2px;
		min-width: 18px;
		height: 18px;
		padding: 0 4px;
		display: flex;
		align-items: center;
		justify-content: center;
		background-color: var(--tmgr-blue, #2563eb);
		color: white;
		font-size: 10px;
		font-weight: 600;
		border-radius: 9px;
		line-height: 1;
	}

	.active-agents-dropdown {
		width: 340px;
		max-height: 520px;
		padding: 0;
		overflow: hidden;
		display: flex;
		flex-direction: column;
	}

	.active-agents-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.875rem 1rem;
	}

	.active-agents-title {
		font-size: 1rem;
		font-weight: 600;
		margin: 0;
	}

	.refresh-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 0.25rem;
		background: transparent;
		border: none;
		color: currentColor;
		cursor: pointer;
		border-radius: 0.25rem;
		opacity: 0.7;
		transition: opacity 0.2s;

		&:hover:not(:disabled) {
			opacity: 1;
		}

		&:disabled {
			cursor: not-allowed;
		}
	}

	.active-agents-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.75rem;
		padding: 2rem;
		font-size: 0.875rem;
		opacity: 0.8;
	}

	.active-agents-list {
		max-height: 420px;
		overflow-y: auto;
		padding: 0.25rem;
	}

	.active-agent-item {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 0.5rem 0.625rem;
		cursor: pointer;
		border-radius: 0.375rem;
	}

	.agent-status-dot {
		flex-shrink: 0;
		width: 8px;
		height: 8px;
		border-radius: 50%;

		&.is-running {
			background-color: #22c55e;
			box-shadow: 0 0 6px rgba(34, 197, 94, 0.6);
		}

		&.is-creating {
			background-color: #f59e0b;
		}
	}

	.agent-info {
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
	}

	.agent-label {
		font-size: 0.875rem;
		font-weight: 500;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.agent-meta {
		font-size: 0.75rem;
		opacity: 0.65;
	}

	.agent-jump {
		flex-shrink: 0;
	}
</style>
