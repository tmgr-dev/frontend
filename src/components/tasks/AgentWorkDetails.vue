<template>
	<Dialog :open="open" @update:open="(value) => $emit('update:open', value)">
		<DialogContent
			class="max-h-[85vh] overflow-y-auto rounded-card border border-line bg-surface text-ink sm:max-w-[760px]"
			data-testid="agent-work-details"
		>
			<DialogHeader>
				<DialogTitle class="text-ink">Agent work — details</DialogTitle>
			</DialogHeader>

			<section class="flex flex-col gap-2" data-section="totals">
				<h3 class="text-2xs font-bold uppercase tracking-wide text-ink-subtle">
					Totals
				</h3>
				<dl class="grid grid-cols-2 gap-x-4 gap-y-1 text-sm sm:grid-cols-3">
					<div>
						<dt class="text-2xs text-ink-subtle">You</dt>
						<dd class="font-mono">
							{{ formatWorkDuration(totals.humanSeconds) }}
						</dd>
					</div>
					<div>
						<dt class="text-2xs text-ink-subtle">Agents</dt>
						<dd class="font-mono">
							{{ formatWorkDuration(totals.agentSeconds) }}
						</dd>
					</div>
					<div v-if="waitingSeconds !== null">
						<dt class="text-2xs text-ink-subtle">Agents waiting</dt>
						<dd class="font-mono">{{ formatWorkDuration(waitingSeconds) }}</dd>
					</div>
					<div v-if="aiCost">
						<dt class="text-2xs text-ink-subtle">{{ aiCost.label }}</dt>
						<dd class="font-mono">{{ aiCost.text }}</dd>
					</div>
					<div v-if="humanCost">
						<dt class="text-2xs text-ink-subtle">
							Your time at {{ humanCost.rate }}/h
						</dt>
						<dd class="font-mono">{{ humanCost.text }}</dd>
					</div>
				</dl>
			</section>

			<p
				v-if="state === 'loading'"
				class="text-xs text-ink-subtle"
				data-state="loading"
			>
				Reading local AI usage…
			</p>
			<p
				v-else-if="unsupportedReason"
				class="text-xs text-ink-subtle"
				data-state="unsupported"
			>
				{{ unsupportedReason }}
			</p>
			<p
				v-else-if="state === 'error'"
				class="text-xs text-ink-subtle"
				data-state="error"
			>
				The local usage data could not be read.
			</p>
			<p
				v-else-if="!aggregate"
				class="text-xs text-ink-subtle"
				data-state="empty"
			>
				No local agent usage linked to this task yet.
			</p>

			<template v-if="aggregate">
				<section class="flex flex-col gap-2" data-section="tokens">
					<h3
						class="text-2xs font-bold uppercase tracking-wide text-ink-subtle"
					>
						Tokens
					</h3>
					<ul class="flex flex-wrap gap-x-4 gap-y-1 text-sm">
						<li v-for="row in tokenList" :key="row.key">
							<span class="text-ink-subtle">{{ row.label }}</span>
							<span class="ml-1 font-mono">{{ formatTokens(row.value) }}</span>
							<span v-if="row.pct != null" class="ml-1 text-2xs text-ink-faint"
								>({{ row.pct }}% of input side)</span
							>
						</li>
					</ul>
				</section>

				<section class="flex flex-col gap-2" data-section="agents">
					<h3
						class="text-2xs font-bold uppercase tracking-wide text-ink-subtle"
					>
						Agents
					</h3>
					<ul class="flex max-h-64 flex-col overflow-y-auto text-sm">
						<li v-for="row in flatTree" :key="row.node.id">
							<button
								type="button"
								class="flex w-full flex-wrap items-baseline gap-x-2 rounded px-1 py-0.5 text-left hover:bg-surface-hover"
								:class="selectedId === row.node.id ? 'bg-surface-hover' : ''"
								:style="{ paddingLeft: `${row.depth * 16 + 4}px` }"
								@click="select(row.node.id)"
							>
								<span class="font-semibold">{{ nodeLabel(row.node) }}</span>
								<span class="text-ink-faint">{{
									modelShort(row.node.model)
								}}</span>
								<span
									v-if="row.node.link === 'likely'"
									class="rounded-pill bg-amber-100 px-1.5 text-2xs text-amber-700 dark:bg-amber-900/40 dark:text-amber-300"
									>likely</span
								>
								<span class="ml-auto font-mono text-ink-subtle">{{
									formatMs(row.node.duration_ms)
								}}</span>
								<span
									v-if="row.node.cost_usd > 0"
									class="font-mono text-emerald-600 dark:text-emerald-400"
									>{{ formatUsd(row.node.cost_usd) }}</span
								>
							</button>
						</li>
					</ul>
					<div
						v-if="selected"
						class="flex flex-col gap-1 rounded-card border border-line px-3 py-2 text-xs"
						data-section="node-detail"
					>
						<div class="font-semibold">
							{{ nodeLabel(selected) }} · {{ modelShort(selected.model) }}
							<span v-if="selected.origin" class="font-normal text-ink-faint"
								>· {{ selected.origin }}</span
							>
						</div>
						<div class="text-ink-subtle">
							{{ formatMs(selected.duration_ms) }} ·
							{{ selected.events }} events
						</div>
						<ul class="flex flex-wrap gap-x-3 gap-y-0.5">
							<li v-for="row in selectedTokens" :key="row.key">
								{{ row.label }}
								<span class="font-mono">{{ formatTokens(row.value) }}</span>
								<span v-if="row.pct != null" class="text-ink-faint"
									>({{ row.pct }}%)</span
								>
							</li>
						</ul>
						<div v-if="selected.tools.length" class="text-ink-subtle">
							{{ toolTotals(selected.tools, 8).map(formatTool).join(' · ') }}
						</div>
					</div>
					<p v-if="aggregate.truncated.nodes" class="text-2xs text-ink-faint">
						Some agents are not shown: the tree is capped.
					</p>
				</section>

				<section
					v-if="tools.length || context.mcp.length || context.hooks.length"
					class="flex flex-col gap-2"
					data-section="tools"
				>
					<h3
						class="text-2xs font-bold uppercase tracking-wide text-ink-subtle"
					>
						Tools
					</h3>
					<p v-if="tools.length" class="text-sm">
						{{ tools.map(formatTool).join(' · ') }}
					</p>
					<p v-if="mcp.length" class="text-xs text-ink-subtle">
						<span class="font-semibold text-ink">MCP</span>
						{{
							mcp
								.map(
									(m) =>
										`${m.server} ×${m.calls}${
											m.errors ? ` (${m.errors} errors)` : ''
										}`,
								)
								.join(' · ')
						}}
					</p>
					<p v-if="hooks.length" class="text-xs text-ink-subtle">
						<span class="font-semibold text-ink">Hooks</span>
						{{
							hooks
								.map(
									(h) =>
										`${h.event} ${h.name} ×${h.count} (${formatMs(
											h.duration_ms,
										)})${h.errors ? `, ${h.errors} failed` : ''}`,
								)
								.join(' · ')
						}}
					</p>
				</section>

				<section
					v-if="contextRows.length"
					class="flex flex-col gap-1"
					data-section="context"
				>
					<h3
						class="text-2xs font-bold uppercase tracking-wide text-ink-subtle"
					>
						Context
					</h3>
					<p
						v-for="row in contextRows"
						:key="row.label"
						class="text-xs text-ink-subtle"
					>
						<span class="font-semibold text-ink">{{ row.label }}</span>
						{{ row.text }}
					</p>
				</section>
			</template>

			<section
				v-if="codeRuns.length"
				class="flex flex-col gap-2"
				data-section="code"
			>
				<h3 class="text-2xs font-bold uppercase tracking-wide text-ink-subtle">
					Code
				</h3>
				<div
					v-for="run in codeRuns"
					:key="run.id"
					class="flex flex-col gap-1 text-xs"
				>
					<div
						class="flex flex-wrap items-center gap-x-3 gap-y-1 text-ink-subtle"
					>
						<span v-if="run.branch" class="font-mono">{{ run.branch }}</span>
						<a
							v-if="isHttpUrl(run.pr_url)"
							:href="run.pr_url"
							target="_blank"
							rel="noopener"
							class="text-brand hover:underline"
							>Pull request</a
						>
						<span v-if="run.tests">
							tests
							<span class="text-emerald-600 dark:text-emerald-400"
								>{{ run.tests.passed ?? 0 }} passed</span
							><template v-if="run.tests.failed"
								>,
								<span class="text-red-600 dark:text-red-400"
									>{{ run.tests.failed }} failed</span
								></template
							>
						</span>
					</div>
					<ul v-if="run.commits.length" class="flex flex-col gap-0.5 font-mono">
						<li v-for="commit in run.commits" :key="commit.sha">
							<span class="text-ink">{{ (commit.sha || '').slice(0, 7) }}</span>
							<span
								class="ml-2 rounded-pill bg-zinc-100 px-1.5 text-2xs text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300"
								>agent</span
							>
							<span class="ml-2 text-ink-subtle">{{ commit.message }}</span>
						</li>
					</ul>
				</div>
			</section>
		</DialogContent>
	</Dialog>
</template>

<script>
	import {
		Dialog,
		DialogContent,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { readTaskUsage, scanThenRead } from '@/usage/loadTaskUsage';
	import {
		buildTree,
		contextSummary,
		formatMs,
		formatTokens,
		formatTool,
		formatUsd,
		formatUsdString,
		groupHooks,
		groupMcp,
		modelShort,
		nodeLabel,
		pricedCost,
		tokenRows,
		toolTotals,
	} from '@/usage/taskModel';
	import { formatWorkDuration, isHttpUrl } from '@/utils/agentWork';
	import { computed, defineComponent, ref, watch } from 'vue';

	const list = (items) => items.map((i) => `${i.name} ×${i.count}`).join(' · ');

	const contextRowsOf = (summary) => {
		const rows = [];
		if (summary.skills?.items.length) {
			const extra = summary.skills.available
				? ` (${summary.skills.available} available)`
				: '';
			rows.push({ label: 'Skills', text: list(summary.skills.items) + extra });
		}
		if (summary.plugins)
			rows.push({ label: 'Plugins', text: list(summary.plugins) });
		if (summary.slash_commands)
			rows.push({ label: 'Commands', text: list(summary.slash_commands) });
		if (summary.permission_modes)
			rows.push({
				label: 'Permission modes',
				text: summary.permission_modes
					.map((m) => `${m.mode} ×${m.count}`)
					.join(' · '),
			});
		if (summary.compactions)
			rows.push({ label: 'Compactions', text: String(summary.compactions) });
		return rows;
	};

	const flatten = (tree, out = []) => {
		for (const item of tree) {
			out.push(item);
			flatten(item.children, out);
		}
		return out;
	};

	export default defineComponent({
		name: 'AgentWorkDetails',
		components: { Dialog, DialogContent, DialogHeader, DialogTitle },
		props: {
			open: { type: Boolean, default: false },
			taskId: { type: Number, required: true },
			workspaceId: { type: Number, default: undefined },
			workspaceCode: { type: String, default: '' },
			runs: { type: Array, default: () => [] },
			totals: { type: Object, required: true },
			cost: { type: Object, default: null },
		},
		emits: ['update:open'],
		setup(props) {
			const state = ref('idle');
			const aggregate = ref(null);
			const unsupportedReason = ref(null);
			const selectedId = ref(null);
			let session = 0;

			const request = () => ({
				taskId: props.taskId,
				workspaceId: props.workspaceId,
				workspaceCode: props.workspaceCode,
				ownRuns: props.runs,
			});

			const apply = (usage) => {
				unsupportedReason.value = usage.supported
					? null
					: usage.unsupported_reason;
				aggregate.value = usage.aggregate;
			};

			const load = async () => {
				const current = ++session;
				state.value = 'loading';
				aggregate.value = null;
				selectedId.value = null;
				unsupportedReason.value = null;
				try {
					const { usage, runs } = await readTaskUsage(request());
					if (current !== session) return;
					apply(usage);
					state.value = 'done';
					if (!usage.supported) return;
					const refreshed = await scanThenRead(request(), runs);
					if (current === session && refreshed) apply(refreshed);
				} catch {
					if (current === session) state.value = 'error';
				}
			};

			watch(
				() => [props.open, props.taskId],
				([open]) => {
					if (open) load();
					else session++;
				},
				{ immediate: true },
			);

			const tree = computed(() => buildTree(aggregate.value?.nodes ?? []));
			const flatTree = computed(() => flatten(tree.value));
			const selected = computed(
				() =>
					aggregate.value?.nodes.find((n) => n.id === selectedId.value) ?? null,
			);
			const context = computed(
				() =>
					aggregate.value?.context ?? {
						tools: [],
						mcp: [],
						hooks: [],
					},
			);

			const aiCost = computed(() => {
				if (aggregate.value && pricedCost(aggregate.value) !== null) {
					return {
						label: 'AI (API-equivalent)',
						text: formatUsd(aggregate.value.totals.cost_usd),
					};
				}
				const synced = props.cost?.ai?.synced
					? formatUsdString(props.cost.ai.costUsd)
					: null;
				return synced
					? { label: 'AI (synced, API-equivalent)', text: synced }
					: null;
			});
			const humanCost = computed(() => {
				const text = formatUsdString(props.cost?.human?.cost);
				const rate = formatUsdString(props.cost?.human?.rate?.value);
				return text && rate ? { text, rate } : null;
			});
			const waitingSeconds = computed(
				() => props.cost?.agents?.waitingSeconds ?? null,
			);

			return {
				state,
				aggregate,
				unsupportedReason,
				selectedId,
				select: (id) => {
					selectedId.value = selectedId.value === id ? null : id;
				},
				selected,
				flatTree,
				context,
				aiCost,
				humanCost,
				waitingSeconds,
				tokenList: computed(() =>
					aggregate.value ? tokenRows(aggregate.value.totals.tokens) : [],
				),
				selectedTokens: computed(() =>
					selected.value ? tokenRows(selected.value.tokens) : [],
				),
				tools: computed(() =>
					toolTotals(aggregate.value?.context.tools ?? [], 12),
				),
				mcp: computed(() => groupMcp(aggregate.value?.context.mcp ?? [])),
				hooks: computed(() =>
					groupHooks(aggregate.value?.context.hooks ?? []).slice(0, 8),
				),
				contextRows: computed(() =>
					aggregate.value
						? contextRowsOf(contextSummary(aggregate.value.context))
						: [],
				),
				codeRuns: computed(() =>
					props.runs.filter(
						(run) =>
							run.branch || run.pr_url || run.commits.length || run.tests,
					),
				),
				formatWorkDuration,
				isHttpUrl,
				formatMs,
				formatTokens,
				formatTool,
				formatUsd,
				modelShort,
				nodeLabel,
				toolTotals,
			};
		},
	});
</script>
