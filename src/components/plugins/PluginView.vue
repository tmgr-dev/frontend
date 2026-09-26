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
	<Button
		v-else-if="node.type === 'button'"
		variant="outline"
		size="sm"
		class="w-fit"
		:disabled="running"
		@click="run"
	>
		{{ node.text }}
	</Button>
	<button
		v-else-if="node.type === 'taskLink'"
		type="button"
		class="text-left text-sm text-primary hover:underline"
		@click="openTask"
	>
		{{ node.text }}
	</button>
	<hr v-else-if="node.type === 'divider'" class="border-border" />
</template>

<script lang="ts">
	import { Button } from '@/components/ui/button';
	import { toast } from '@/components/ui/toast';
	import { pluginHost, pluginState } from '@/pluginSystem/state';
	import type { Color, Tone, UiNode } from '@/pluginSystem/uiTree';
	import store from '@/store';
	import { defineComponent, ref, type PropType } from 'vue';

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
	};

	const BARS: Record<Color, string> = {
		gray: 'bg-muted-foreground',
		green: 'bg-emerald-500',
		yellow: 'bg-amber-500',
		red: 'bg-red-500',
		blue: 'bg-sky-500',
	};

	export default defineComponent({
		name: 'PluginView',
		components: { Button },
		props: {
			node: { type: Object as PropType<UiNode>, required: true },
			pluginId: { type: String, required: true },
		},
		setup(props) {
			const running = ref(false);
			const run = async () => {
				if (props.node.type !== 'button') return;
				running.value = true;
				try {
					await pluginHost()?.runCommand(
						props.pluginId,
						props.node.command,
						props.node.args ?? null,
					);
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
			const openTask = () => {
				if (props.node.type === 'taskLink') {
					store.commit('setCurrentTaskIdForModal', props.node.taskId);
				}
			};
			return {
				running,
				run,
				openTask,
				toneClass: (tone: Tone) => TONES[tone],
				colorClass: (color: Color) => COLORS[color],
				barClass: (color: Color) => BARS[color],
			};
		},
	});
</script>
