<template>
	<div
		v-if="node.type === 'stack'"
		:class="[
			'flex gap-3',
			node.direction === 'row'
				? 'flex-row flex-wrap items-stretch'
				: 'flex-col',
		]"
	>
		<PluginView
			v-for="(child, index) in node.children"
			:key="index"
			:node="child"
			:plugin-id="pluginId"
		/>
	</div>
	<component
		:is="`h${node.level + 1}`"
		v-else-if="node.type === 'heading'"
		:class="[
			'font-semibold text-foreground',
			node.level === 1 ? 'text-xl' : node.level === 2 ? 'text-base' : 'text-sm',
		]"
	>
		{{ node.text }}
	</component>
	<p
		v-else-if="node.type === 'text'"
		:class="['text-sm', toneClass(node.tone)]"
	>
		{{ node.text }}
	</p>
	<span
		v-else-if="node.type === 'badge'"
		:class="[
			'inline-flex w-fit items-center rounded px-1.5 py-0.5 text-2xs font-semibold tabular-nums',
			colorClass(node.color),
		]"
	>
		{{ node.text }}
	</span>
	<div
		v-else-if="node.type === 'stat'"
		class="min-w-[8rem] flex-1 rounded-md border border-border bg-card px-3 py-2"
	>
		<div class="text-2xs uppercase tracking-wide text-muted-foreground">
			{{ node.label }}
		</div>
		<div :class="['text-lg font-semibold tabular-nums', toneClass(node.tone)]">
			{{ node.value }}
		</div>
	</div>
	<div
		v-else-if="node.type === 'progress'"
		class="h-2 w-full overflow-hidden rounded-full bg-muted"
	>
		<div
			:class="['h-full rounded-full', barClass(node.color)]"
			:style="{ width: `${Math.min(100, node.value * 100)}%` }"
		/>
	</div>
	<ul v-else-if="node.type === 'list'" class="flex flex-col gap-1.5">
		<li v-for="(item, index) in node.items" :key="index">
			<PluginView :node="item" :plugin-id="pluginId" />
		</li>
	</ul>
	<div
		v-else-if="node.type === 'table'"
		class="overflow-x-auto rounded-md border border-border"
	>
		<table class="w-full text-sm">
			<thead
				class="bg-muted/50 text-left text-2xs uppercase text-muted-foreground"
			>
				<tr>
					<th
						v-for="column in node.columns"
						:key="column.key"
						class="px-3 py-2 font-medium"
					>
						{{ column.title }}
					</th>
				</tr>
			</thead>
			<tbody>
				<tr
					v-for="(row, index) in node.rows"
					:key="row.taskId ?? index"
					class="border-t border-border"
				>
					<td
						v-for="column in node.columns"
						:key="column.key"
						class="px-3 py-2"
					>
						<PluginView
							v-if="row.cells[column.key]"
							:node="row.cells[column.key]"
							:plugin-id="pluginId"
						/>
					</td>
				</tr>
			</tbody>
		</table>
	</div>
	<AlertDialog v-else-if="node.type === 'button'" v-model:open="confirmOpen">
		<Button
			variant="outline"
			size="sm"
			class="w-fit"
			:disabled="running"
			@click="handleButtonClick"
		>
			{{ node.text }}
		</Button>
		<AlertDialogContent v-if="node.confirm">
			<AlertDialogHeader>
				<AlertDialogTitle>{{ pluginName }}</AlertDialogTitle>
				<AlertDialogDescription>{{ node.confirm }}</AlertDialogDescription>
			</AlertDialogHeader>
			<AlertDialogFooter>
				<AlertDialogCancel>Cancel</AlertDialogCancel>
				<AlertDialogAction @click="run">Continue</AlertDialogAction>
			</AlertDialogFooter>
		</AlertDialogContent>
	</AlertDialog>
	<button
		v-else-if="node.type === 'taskLink'"
		type="button"
		class="text-left text-sm text-primary hover:underline"
		@click="openTask"
	>
		{{ node.text }}
	</button>
	<hr v-else-if="node.type === 'divider'" class="border-border" />
	<div v-else-if="node.type === 'copyable'" class="flex items-center gap-2">
		<span v-if="node.label" class="text-2xs text-muted-foreground">{{
			node.label
		}}</span>
		<code
			class="rounded bg-muted px-1.5 py-0.5 font-mono text-xs text-foreground"
			>{{ node.text }}</code
		>
		<button
			type="button"
			class="text-muted-foreground transition-colors hover:text-foreground"
			:title="isCopied ? 'Copied' : 'Copy'"
			@click="copyText"
		>
			<Check
				v-if="isCopied"
				class="h-3.5 w-3.5 text-emerald-600 dark:text-emerald-400"
			/>
			<Copy v-else class="h-3.5 w-3.5" />
		</button>
	</div>
	<button
		v-else-if="node.type === 'link'"
		type="button"
		class="inline-flex w-fit items-center gap-1 text-left text-sm text-primary hover:underline"
		@click="openLinkNode"
	>
		<span>{{ node.text }}</span>
		<span class="text-2xs text-muted-foreground">({{ node.host }})</span>
	</button>
	<span
		v-else-if="node.type === 'timeAgo'"
		class="text-2xs tabular-nums text-muted-foreground"
		:title="node.at"
	>
		{{ timeAgoText }}
	</span>
	<span
		v-else-if="node.type === 'dueTime'"
		:class="['text-2xs tabular-nums', dueTimeClass]"
		:title="node.at"
	>
		{{ dueTimeText }}
	</span>
	<dl
		v-else-if="node.type === 'keyValue'"
		class="grid grid-cols-[auto,1fr] gap-x-3 gap-y-1.5 text-sm"
	>
		<template v-for="(item, index) in node.items" :key="index">
			<dt class="text-muted-foreground">{{ item.key }}</dt>
			<dd>
				<PluginView :node="item.value" :plugin-id="pluginId" />
			</dd>
		</template>
	</dl>
</template>

<script lang="ts">
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
	import { Button } from '@/components/ui/button';
	import { toast } from '@/components/ui/toast';
	import { useCopyToClipboard } from '@/composable/useCopyToClipboard';
	import { usePluginTicker } from '@/composable/usePluginTicker';
	import { formatDueTime, formatTimeAgo } from '@/pluginSystem/relativeTime';
	import { pluginHost, pluginState } from '@/pluginSystem/state';
	import type { Color, Tone, UiNode } from '@/pluginSystem/uiTree';
	import store from '@/store';
	import { Check, Copy } from 'lucide-vue-next';
	import { computed, defineComponent, ref, type PropType } from 'vue';

	const TONES: Record<Tone, string> = {
		default: 'text-foreground',
		muted: 'text-muted-foreground',
		success: 'text-emerald-600 dark:text-emerald-400',
		warning: 'text-amber-600 dark:text-amber-400',
		danger: 'text-red-600 dark:text-red-400',
	};

	const COLORS: Record<Color, string> = {
		gray: 'bg-muted text-muted-foreground',
		green: 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-300',
		yellow: 'bg-amber-500/15 text-amber-700 dark:text-amber-300',
		red: 'bg-red-500/15 text-red-700 dark:text-red-300',
		blue: 'bg-sky-500/15 text-sky-700 dark:text-sky-300',
		purple: 'bg-purple-500/15 text-purple-700 dark:text-purple-300',
		orange: 'bg-orange-500/15 text-orange-700 dark:text-orange-300',
	};

	const BARS: Record<Color, string> = {
		gray: 'bg-muted-foreground',
		green: 'bg-emerald-500',
		yellow: 'bg-amber-500',
		red: 'bg-red-500',
		blue: 'bg-sky-500',
		purple: 'bg-purple-500',
		orange: 'bg-orange-500',
	};

	export default defineComponent({
		name: 'PluginView',
		components: {
			AlertDialog,
			AlertDialogAction,
			AlertDialogCancel,
			AlertDialogContent,
			AlertDialogDescription,
			AlertDialogFooter,
			AlertDialogHeader,
			AlertDialogTitle,
			Button,
			Check,
			Copy,
		},
		props: {
			node: { type: Object as PropType<UiNode>, required: true },
			pluginId: { type: String, required: true },
		},
		setup(props) {
			const running = ref(false);
			const confirmOpen = ref(false);
			const run = async () => {
				if (props.node.type !== 'button') return;
				confirmOpen.value = false;
				running.value = true;
				try {
					// node.args can be a reactive proxy by the time it reaches here (the tree lives in a
					// reactive ref/record); postMessage to the plugin worker needs a plain, cloneable value.
					const args =
						props.node.args === undefined
							? null
							: JSON.parse(JSON.stringify(props.node.args));
					await pluginHost()?.runCommand(props.pluginId, props.node.command, args);
				} catch (error) {
					if (pluginState.plugins[props.pluginId]?.status === 'crashed') return;
					toast({
						title: 'The plugin command failed',
						description: error instanceof Error ? error.message : String(error),
						variant: 'destructive',
					});
				} finally {
					running.value = false;
				}
			};
			const handleButtonClick = () => {
				if (props.node.type !== 'button') return;
				if (props.node.confirm) confirmOpen.value = true;
				else void run();
			};
			const openTask = () => {
				if (props.node.type === 'taskLink') {
					store.commit('setCurrentTaskIdForModal', props.node.taskId);
				}
			};
			const openLinkNode = () => {
				if (props.node.type === 'link') {
					pluginHost()?.openLink(props.pluginId, props.node.url);
				}
			};
			const [copy, copiedText] = useCopyToClipboard();
			const copyText = () => {
				if (props.node.type === 'copyable') void copy(props.node.text);
			};
			const isCopied = computed(
				() =>
					props.node.type === 'copyable' && copiedText.value === props.node.text,
			);
			const tick = usePluginTicker();
			const timeAgoText = computed(() =>
				props.node.type === 'timeAgo'
					? formatTimeAgo(props.node.at, tick.value)
					: '',
			);
			const due = computed(() =>
				props.node.type === 'dueTime'
					? formatDueTime(props.node.at, tick.value)
					: null,
			);
			const dueTimeText = computed(() => due.value?.text ?? '');
			const dueTimeClass = computed(() =>
				due.value?.urgency === 'overdue'
					? 'text-red-600 dark:text-red-400'
					: due.value?.urgency === 'soon'
					? 'text-amber-600 dark:text-amber-400'
					: 'text-foreground',
			);
			const pluginName = computed(
				() => pluginState.plugins[props.pluginId]?.manifest.name ?? props.pluginId,
			);
			return {
				running,
				confirmOpen,
				run,
				handleButtonClick,
				openTask,
				openLinkNode,
				copyText,
				isCopied,
				timeAgoText,
				dueTimeText,
				dueTimeClass,
				pluginName,
				toneClass: (tone: Tone) => TONES[tone],
				colorClass: (color: Color) => COLORS[color],
				barClass: (color: Color) => BARS[color],
			};
		},
	});
</script>
