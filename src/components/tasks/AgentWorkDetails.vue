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
				<h3 class="text-2xs font-bold tracking-wide text-ink-subtle uppercase">
					Totals
				</h3>
				<dl class="grid grid-cols-2 gap-x-4 gap-y-1 text-sm sm:grid-cols-3">
					<div>
						<dt class="text-2xs text-ink-subtle">You</dt>
						<dd class="font-mono">
							{{ formatWorkDuration(totals.humanSeconds) }}
						</dd>
					</div>
					<div v-for="row in timeList" :key="row.key">
						<dt class="text-2xs text-ink-subtle">{{ row.label }}</dt>
						<dd class="font-mono">{{ formatWorkDuration(row.seconds) }}</dd>
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
				v-if="notice"
				class="rounded-card border border-line px-3 py-2 text-xs text-ink-subtle"
				data-state="notice"
			>
				{{ notice }}
			</p>
			<p
				v-if="unpriced"
				class="text-xs text-amber-700 dark:text-amber-300"
				data-state="unpriced"
			>
				{{ unpriced }}
			</p>
			<p
				v-if="!aggregate && state === 'loading'"
				class="text-xs text-ink-subtle"
				data-state="loading"
			>
				Reading local AI usage…
			</p>
			<p
				v-else-if="!aggregate && unsupportedReason"
				class="text-xs text-ink-subtle"
				data-state="unsupported"
			>
				{{ unsupportedReason }}
			</p>
			<p
				v-else-if="!aggregate && state === 'error'"
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
				No agent usage linked to this task yet.
			</p>

			<template v-if="aggregate">
				<section class="flex flex-col gap-2" data-section="tokens">
					<h3
						class="text-2xs font-bold tracking-wide text-ink-subtle uppercase"
					>
						Tokens
						<span class="font-normal normal-case">· {{ sourceLabel }}</span>
					</h3>
					<ul class="flex flex-wrap gap-x-4 gap-y-1 text-sm">
						<li v-for="row in tokenList" :key="row.key">
							<span class="text-ink-subtle">{{ row.label }}</span>
							<span class="ml-1 font-mono">{{ formatTokens(row.value) }}</span>
							<span v-if="row.pct != null" class="ml-1 text-2xs text-ink-subtle"
								>({{ row.pct }}% of input side)</span
							>
						</li>
					</ul>
					<ul
						v-if="rates.length"
						class="flex flex-col gap-0.5 text-xs text-ink-subtle"
						data-section="rates"
					>
						<li v-for="row in rates" :key="row.model">
							<span class="font-semibold text-ink">Rates {{ row.model }}</span>
							{{ row.text }}
						</li>
					</ul>
				</section>

				<section
					v-if="flatTree.length"
					class="flex flex-col gap-2"
					data-section="agents"
				>
					<h3
						class="text-2xs font-bold tracking-wide text-ink-subtle uppercase"
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
								<span class="text-ink-subtle">{{
									modelShort(row.node.model)
								}}</span>
								<span
									v-if="row.node.link === 'likely'"
									class="rounded-pill bg-amber-100 px-1.5 text-2xs text-amber-700 dark:bg-amber-900/40 dark:text-amber-300"
									>likely</span
								>
								<span
									class="ml-auto font-mono text-ink-subtle"
									:title="nodeTime(row.node).title"
									>{{ nodeTime(row.node).text }}</span
								>
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
							<span v-if="selected.origin" class="font-normal text-ink-subtle"
								>· {{ selected.origin }}</span
							>
						</div>
						<div class="text-ink-subtle">
							<span :title="nodeTime(selected).title">{{
								nodeTime(selected).text
							}}</span>
							·
							{{ selected.events }} events
						</div>
						<ul class="flex flex-wrap gap-x-3 gap-y-0.5">
							<li v-for="row in selectedTokens" :key="row.key">
								{{ row.label }}
								<span class="font-mono">{{ formatTokens(row.value) }}</span>
								<span v-if="row.pct != null" class="text-ink-subtle"
									>({{ row.pct }}%)</span
								>
							</li>
						</ul>
						<div v-if="selected.tools.length" class="text-ink-subtle">
							{{ toolTotals(selected.tools, 8).map(formatTool).join(' · ') }}
						</div>
					</div>
					<p v-if="aggregate.truncated.nodes" class="text-2xs text-ink-subtle">
						Some agents are not shown: the tree is capped.
					</p>
				</section>

				<section
					v-if="tools.length || context.mcp.length || context.hooks.length"
					class="flex flex-col gap-2"
					data-section="tools"
				>
					<h3
						class="text-2xs font-bold tracking-wide text-ink-subtle uppercase"
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
						class="text-2xs font-bold tracking-wide text-ink-subtle uppercase"
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
				<h3 class="text-2xs font-bold tracking-wide text-ink-subtle uppercase">
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
							rel="noopener noreferrer"
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
					<ul
						v-if="releaseRows(run).length"
						class="flex flex-col gap-0.5"
						data-section="releases"
					>
						<li v-for="(rel, i) in releaseRows(run)" :key="i">
							<span class="text-ink-subtle">Release</span>
							<span class="ml-1 font-mono">{{ rel.label }}</span>
							<a
								v-if="rel.ciUrl"
								:href="rel.ciUrl"
								target="_blank"
								rel="noopener noreferrer"
								class="ml-2 text-brand hover:underline"
								>CI</a
							>
							<span
								v-if="rel.digest"
								class="ml-2 font-mono text-ink-subtle"
								:title="rel.digestTitle"
								>{{ rel.digest }}</span
							>
						</li>
					</ul>
					<p v-if="deployRow(run)" data-section="deploy">
						<span class="text-ink-subtle">Deploy</span>
						<span class="ml-1 font-mono">{{ deployRow(run).label }}</span>
						<a
							v-if="deployRow(run).url"
							:href="deployRow(run).url"
							target="_blank"
							rel="noopener noreferrer"
							class="ml-2 text-brand hover:underline"
							>Open</a
						>
					</p>
					<ul
						v-if="run.checks && run.checks.length"
						class="flex flex-col gap-0.5"
						data-section="checks"
					>
						<li
							v-for="(check, i) in run.checks"
							:key="i"
							:title="check.command || undefined"
						>
							<span class="text-ink-subtle">Check</span>
							<span class="ml-1">{{ check.name }} — {{ check.result }}</span>
						</li>
					</ul>
				</div>
				<div
					v-for="group in commitGroupList"
					:key="group.key"
					class="flex flex-col gap-0.5 text-xs"
					data-section="commits"
				>
					<div v-if="group.header" class="font-mono text-ink-subtle">
						{{ group.header }}
					</div>
					<ul class="flex flex-col gap-0.5 font-mono">
						<li v-for="commit in group.commits" :key="commit.sha">
							<a
								v-if="commit.url"
								:href="commit.url"
								target="_blank"
								rel="noopener noreferrer"
								class="text-brand hover:underline"
								>{{ commit.short }}</a
							>
							<span v-else class="text-ink">{{ commit.short }}</span>
							<span
								class="ml-2 rounded-pill bg-zinc-100 px-1.5 text-2xs text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300"
								:title="
									commit.source === 'git'
										? 'Found in git, not reported by the agent'
										: undefined
								"
								>{{ commit.source }}</span
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
		pickUsage,
		ratesRows,
		serverAggregate,
		unpricedWarning,
	} from '@/usage/serverUsage';
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
		nodeTime,
		pricedCost,
		tokenRows,
		toolTotals,
	} from '@/usage/taskModel';
	import {
		commitGroups,
		formatDeploy,
		formatRelease,
		formatWorkDuration,
		isHttpUrl,
		timeRows,
	} from '@/utils/agentWork';
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
			costState: { type: String, default: 'ready' },
		},
		emits: ['update:open'],
		setup(props) {
			const state = ref('idle');
			const localAggregate = ref(null);
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
				localAggregate.value = usage.aggregate;
			};

			const load = async () => {
				const current = ++session;
				state.value = 'loading';
				localAggregate.value = null;
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

			const picked = computed(() =>
				pickUsage({
					synced: serverAggregate(props.cost?.ai),
					local: localAggregate.value,
					serverState: props.costState,
				}),
			);
			const aggregate = computed(() => picked.value.aggregate);
			const notice = computed(() => picked.value.notice);
			const sourceLabel = computed(() =>
				picked.value.source === 'synced' ? 'Synced to TMGR' : 'Local only',
			);
			const unpriced = computed(() =>
				unpricedWarning(aggregate.value?.totals.unpriced_events),
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
				if (!aggregate.value || pricedCost(aggregate.value) === null)
					return null;
				return {
					label:
						picked.value.source === 'synced'
							? 'AI, synced (API-equivalent)'
							: 'AI, local only (API-equivalent)',
					text: formatUsd(aggregate.value.totals.cost_usd),
				};
			});
			const humanCost = computed(() => {
				const text = formatUsdString(props.cost?.human?.cost);
				const rate = formatUsdString(props.cost?.human?.rate?.value);
				return text && rate ? { text, rate } : null;
			});
			const timeList = computed(() =>
				timeRows(props.totals.agentSeconds, props.cost?.agents ?? null),
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
				timeList,
				notice,
				sourceLabel,
				unpriced,
				rates: computed(() => ratesRows(props.cost?.ai?.rates)),
				commitGroupList: computed(() => commitGroups(props.runs)),
				releaseRows: (run) => (run.releases ?? []).map(formatRelease),
				deployRow: (run) => (run.deploy ? formatDeploy(run.deploy) : null),
				nodeTime,
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
							run.branch ||
							run.pr_url ||
							run.commits.length ||
							run.tests ||
							run.releases?.length ||
							run.deploy ||
							run.checks?.length,
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
