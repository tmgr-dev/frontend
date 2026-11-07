<template>
	<div
		class="flex min-w-0 flex-col gap-2 text-sm leading-relaxed text-foreground"
	>
		<template v-for="(block, index) in blocks" :key="index">
			<component
				:is="`h${block.depth + 1}`"
				v-if="block.type === 'heading'"
				:class="[
					'font-semibold',
					block.depth === 1
						? 'text-xl'
						: block.depth === 2
							? 'text-base'
							: 'text-sm',
				]"
			>
				<PluginMarkdownInline :nodes="block.children" :plugin-id="pluginId" />
			</component>
			<p v-else-if="block.type === 'paragraph'" class="break-words">
				<PluginMarkdownInline :nodes="block.children" :plugin-id="pluginId" />
			</p>
			<component
				:is="block.ordered ? 'ol' : 'ul'"
				v-else-if="block.type === 'list'"
				:start="block.ordered ? block.start : undefined"
				:class="[
					'flex flex-col gap-1 pl-5',
					block.ordered ? 'list-decimal' : 'list-disc',
				]"
			>
				<li
					v-for="(item, itemIndex) in block.items"
					:key="itemIndex"
					:class="item.checked !== undefined && 'list-none'"
				>
					<div
						v-if="item.checked !== undefined"
						class="flex items-start gap-1.5"
					>
						<component
							:is="item.checked ? 'CheckSquare' : 'Square'"
							class="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground"
							:aria-label="item.checked ? 'Done' : 'Not done'"
						/>
						<PluginMarkdown
							class="min-w-0 flex-1"
							:blocks="item.blocks"
							:plugin-id="pluginId"
						/>
					</div>
					<PluginMarkdown v-else :blocks="item.blocks" :plugin-id="pluginId" />
				</li>
			</component>
			<div
				v-else-if="block.type === 'table'"
				class="overflow-x-auto rounded-md border border-border"
			>
				<table class="w-full text-sm">
					<thead class="bg-muted/50 text-left">
						<tr>
							<th
								v-for="(cell, cellIndex) in block.header"
								:key="cellIndex"
								:class="[
									'px-3 py-2 font-medium',
									alignClass(block.align[cellIndex]),
								]"
							>
								<PluginMarkdownInline :nodes="cell" :plugin-id="pluginId" />
							</th>
						</tr>
					</thead>
					<tbody>
						<tr
							v-for="(row, rowIndex) in block.rows"
							:key="rowIndex"
							class="border-t border-border"
						>
							<td
								v-for="(cell, cellIndex) in row"
								:key="cellIndex"
								:class="[
									'px-3 py-2 align-top',
									alignClass(block.align[cellIndex]),
								]"
							>
								<PluginMarkdownInline :nodes="cell" :plugin-id="pluginId" />
							</td>
						</tr>
					</tbody>
				</table>
			</div>
			<pre
				v-else-if="block.type === 'code'"
				class="rounded-md bg-muted p-3 font-mono text-xs break-words whitespace-pre-wrap text-foreground"
				>{{ block.text }}</pre>
			<blockquote
				v-else-if="block.type === 'blockquote'"
				class="border-l-2 border-border pl-3 text-muted-foreground"
			>
				<PluginMarkdown :blocks="block.blocks" :plugin-id="pluginId" />
			</blockquote>
			<hr v-else-if="block.type === 'hr'" class="border-border" />
		</template>
	</div>
</template>

<script lang="ts">
	import type { MdBlock } from '@/pluginSystem/uiTree';
	import { CheckSquare, Square } from 'lucide-vue-next';
	import { defineComponent, type PropType } from 'vue';
	import PluginMarkdownInline from './PluginMarkdownInline.vue';

	export default defineComponent({
		name: 'PluginMarkdown',
		components: { PluginMarkdownInline, CheckSquare, Square },
		props: {
			blocks: { type: Array as PropType<MdBlock[]>, required: true },
			pluginId: { type: String, required: true },
		},
		setup() {
			return {
				alignClass: (align: 'left' | 'center' | 'right' | null | undefined) =>
					align === 'center'
						? 'text-center'
						: align === 'right'
							? 'text-right'
							: 'text-left',
			};
		},
	});
</script>
