<template>
	<div v-if="node.type === 'stack'" :class="stackClasses(node)">
		<PluginView
			v-for="(child, index) in node.children"
			:key="index"
			:node="child"
			:plugin-id="pluginId"
		/>
	</div>
	<div
		v-else-if="node.type === 'card'"
		:class="cardOuterClasses(node)"
		:role="node.onClick ? 'button' : undefined"
		:tabindex="node.onClick ? 0 : undefined"
		:aria-label="node.onClick ? node.label : undefined"
		@click="handleCardClick"
		@keydown="handleCardKeydown"
	>
		<div
			v-if="node.accent"
			:class="['h-[3px] w-full', barClass(node.accent)]"
		/>
		<div :class="cardInnerClasses(node)">
			<PluginView
				v-for="(child, index) in node.children"
				:key="index"
				:node="child"
				:plugin-id="pluginId"
			/>
		</div>
	</div>
	<div v-else-if="node.type === 'grid'" class="w-full min-w-0 overflow-x-auto">
		<div :class="gridInnerClasses(node)" :style="gridTemplateStyle(node)">
			<PluginView
				v-for="(child, index) in node.children"
				:key="index"
				:node="child"
				:plugin-id="pluginId"
			/>
		</div>
	</div>
	<template v-else-if="node.type === 'menu'">
		<DropdownMenu>
			<DropdownMenuTrigger as-child>
				<button
					type="button"
					class="inline-flex h-7 w-7 shrink-0 items-center justify-center gap-1 rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
					:aria-label="node.label ?? 'More actions'"
				>
					<MoreHorizontal class="h-4 w-4" />
					<span v-if="node.label" class="text-xs">{{ node.label }}</span>
				</button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end">
				<DropdownMenuItem
					v-for="(item, index) in node.items"
					:key="index"
					@select="handleMenuSelect(item)"
				>
					{{ item.text }}
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
		<AlertDialog v-model:open="menuConfirmOpen">
			<AlertDialogContent v-if="menuConfirmItem">
				<AlertDialogHeader>
					<AlertDialogTitle
						>Plugin {{ pluginName }}:
						{{ menuConfirmItem.text }}</AlertDialogTitle
					>
					<AlertDialogDescription>{{
						menuConfirmItem.confirm
					}}</AlertDialogDescription>
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel>Cancel</AlertDialogCancel>
					<AlertDialogAction @click="runMenuConfirm"
						>Continue</AlertDialogAction
					>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
	</template>
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
		:class="[
			'text-sm',
			toneClass(node.tone),
			node.breaks && 'break-words whitespace-pre-line',
		]"
	>
		<template v-if="node.spans">
			<span
				v-for="(span, index) in node.spans"
				:key="index"
				:class="[
					span.bold && 'font-semibold',
					span.italic && 'italic',
					span.strike && 'line-through',
					span.code &&
						'rounded bg-muted px-1 py-0.5 font-mono text-[0.85em] text-foreground',
				]"
				>{{ span.text }}</span
			>
		</template>
		<template v-else>{{ node.text }}</template>
	</p>
	<PluginMarkdown
		v-else-if="node.type === 'markdown'"
		:blocks="node.blocks"
		:plugin-id="pluginId"
	/>
	<div
		v-else-if="node.type === 'code'"
		class="min-w-0 rounded-md border border-border bg-muted"
	>
		<div
			class="flex items-center justify-between gap-2 px-3 pt-1.5 text-2xs text-muted-foreground"
		>
			<span>{{ node.lang }}</span>
			<button
				type="button"
				class="transition-colors hover:text-foreground"
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
		<pre
			:class="[
				'p-3 font-mono text-xs text-foreground',
				node.wrap
					? 'break-words whitespace-pre-wrap'
					: 'overflow-x-auto whitespace-pre',
			]"
			>{{ node.text }}</pre>
	</div>
	<AppTooltip v-else-if="node.type === 'badge'" :content="node.tooltip">
		<component
			:is="node.command ? 'button' : 'span'"
			:type="node.command ? 'button' : undefined"
			:class="[
				'inline-flex w-fit items-center gap-1 rounded px-1.5 py-0.5 text-2xs font-semibold tabular-nums',
				colorClass(node.color),
				node.command && 'cursor-pointer transition-colors hover:opacity-80',
			]"
			@click="node.command ? runNodeCommand(node) : undefined"
		>
			<component
				:is="iconComponent(node.icon)"
				v-if="node.icon"
				class="h-3 w-3 shrink-0"
			/>
			{{ node.text }}
		</component>
	</AppTooltip>
	<component
		:is="iconComponent(node.name)"
		v-else-if="node.type === 'icon'"
		:class="['h-4 w-4 shrink-0', toneClass(node.tone)]"
		aria-hidden="true"
	/>
	<div
		v-else-if="node.type === 'buttonGroup'"
		class="inline-flex w-fit flex-wrap items-center gap-1.5"
	>
		<PluginView
			v-for="(child, index) in node.children"
			:key="index"
			:node="child"
			:plugin-id="pluginId"
		/>
	</div>
	<div v-else-if="node.type === 'collapse'" class="flex flex-col gap-2">
		<button
			v-if="node.title"
			type="button"
			class="flex w-fit items-center gap-1 text-sm font-medium text-foreground transition-colors hover:text-primary"
			:aria-expanded="collapseOpen"
			@click="collapseOpen = !collapseOpen"
		>
			<ChevronRight
				:class="[
					'h-4 w-4 shrink-0 transition-transform',
					collapseOpen && 'rotate-90',
				]"
			/>
			<span>{{ node.title }}</span>
		</button>
		<template v-if="!node.title || collapseOpen">
			<PluginView
				v-for="(child, index) in collapseChildren"
				:key="index"
				:node="child"
				:plugin-id="pluginId"
			/>
			<button
				v-if="collapseToggleText"
				type="button"
				class="w-fit text-xs text-primary hover:underline"
				@click="collapseShowAll = !collapseShowAll"
			>
				{{ collapseToggleText }}
			</button>
		</template>
	</div>
	<div v-else-if="node.type === 'tabs'" class="flex flex-col gap-3">
		<div role="tablist" class="flex flex-wrap gap-1 border-b border-border">
			<button
				v-for="tab in node.tabs"
				:key="tab.id"
				type="button"
				role="tab"
				:aria-selected="tab.id === activeTabId"
				:class="[
					'-mb-px inline-flex items-center gap-1.5 border-b-2 px-3 py-1.5 text-sm transition-colors',
					tab.id === activeTabId
						? 'border-primary font-medium text-foreground'
						: 'border-transparent text-muted-foreground hover:text-foreground',
				]"
				@click="selectedTabId = tab.id"
			>
				{{ tab.title }}
				<span
					v-if="tab.badge"
					class="rounded bg-muted px-1.5 py-0.5 text-2xs text-muted-foreground tabular-nums"
					>{{ tab.badge }}</span
				>
			</button>
		</div>
		<div role="tabpanel" class="flex flex-col gap-3">
			<PluginView
				v-for="(child, index) in activeTabChildren"
				:key="`${activeTabId}-${index}`"
				:node="child"
				:plugin-id="pluginId"
			/>
		</div>
	</div>
	<div
		v-else-if="node.type === 'segmented'"
		role="radiogroup"
		class="inline-flex w-fit rounded-md bg-muted p-0.5"
	>
		<button
			v-for="option in node.options"
			:key="option.value"
			type="button"
			role="radio"
			:aria-checked="option.value === node.value"
			:disabled="running"
			:class="[
				'rounded px-3 py-1 text-xs font-medium transition-colors disabled:opacity-50',
				option.value === node.value
					? 'bg-background text-foreground shadow-xs'
					: 'text-muted-foreground hover:text-foreground',
			]"
			@click="runSegmented(option.value)"
		>
			{{ option.text }}
		</button>
	</div>
	<Input
		v-else-if="node.type === 'input'"
		:model-value="inputText"
		type="text"
		autocomplete="off"
		:placeholder="node.placeholder"
		:maxlength="500"
		class="h-8 w-full max-w-sm text-xs"
		@update:model-value="onInputText"
		@focus="inputFocused = true"
		@blur="inputFocused = false"
		@keydown.enter="onInputEnter"
	/>
	<select
		v-else-if="node.type === 'select'"
		:value="node.value ?? ''"
		autocomplete="off"
		class="h-8 w-fit max-w-full rounded-md border border-input bg-background px-2 text-xs text-foreground"
		@change="onSelectChange"
	>
		<option v-if="node.value === null" value="" disabled>
			{{ node.placeholder ?? 'Select' }}
		</option>
		<option
			v-for="option in node.options"
			:key="option.value"
			:value="option.value"
		>
			{{ option.text }}
		</option>
	</select>
	<label
		v-else-if="node.type === 'checkbox'"
		class="inline-flex w-fit cursor-pointer items-center gap-2 text-sm text-foreground"
	>
		<input
			type="checkbox"
			autocomplete="off"
			:checked="node.checked"
			class="h-4 w-4 rounded border-input accent-primary"
			@change="onCheckboxChange"
		/>
		<span>{{ node.label }}</span>
	</label>
	<button
		v-else-if="node.type === 'stat' && node.command"
		type="button"
		:class="statClasses(true)"
		@click="runNodeCommand(node)"
	>
		<div class="text-2xs tracking-wide text-muted-foreground uppercase">
			{{ node.label }}
		</div>
		<div :class="['text-lg font-semibold tabular-nums', toneClass(node.tone)]">
			{{ node.value }}
		</div>
	</button>
	<div v-else-if="node.type === 'stat'" :class="statClasses(false)">
		<div class="text-2xs tracking-wide text-muted-foreground uppercase">
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
				class="bg-muted/50 text-left text-2xs text-muted-foreground uppercase"
			>
				<tr>
					<th v-if="tableHasExpand" class="w-8 px-2 py-2" />
					<th
						v-for="column in node.columns"
						:key="column.key"
						:class="[
							'px-3 py-2 font-medium',
							column.align ? columnAlignClass[column.align] : '',
							column.width ? columnWidthClass[column.width] : '',
						]"
						:aria-sort="column.sortable ? ariaSort(column.key) : undefined"
					>
						<button
							v-if="column.sortable"
							type="button"
							class="inline-flex items-center gap-1 uppercase hover:text-foreground"
							@click="toggleSort(column.key)"
						>
							{{ column.title }}
							<component
								:is="sortIcon(column.key)"
								class="h-3 w-3 shrink-0"
								aria-hidden="true"
							/>
						</button>
						<template v-else>{{ column.title }}</template>
					</th>
				</tr>
			</thead>
			<tbody>
				<template
					v-for="index in tableOrder"
					:key="node.rows[index].taskId ?? index"
				>
					<tr
						:class="[
							'border-t border-border',
							node.rows[index].color
								? rowTintClass(node.rows[index].color)
								: '',
							node.rows[index].onClick
								? 'cursor-pointer hover:bg-muted/40 focus-visible:ring-1 focus-visible:ring-ring focus-visible:outline-none'
								: '',
						]"
						:tabindex="node.rows[index].onClick ? 0 : undefined"
						@click="handleRowClick($event, node.rows[index])"
						@keydown="handleRowKeydown($event, node.rows[index])"
					>
						<td v-if="tableHasExpand" class="w-8 px-2 py-2">
							<button
								v-if="node.rows[index].expand"
								type="button"
								class="flex h-5 w-5 items-center justify-center rounded text-muted-foreground hover:bg-muted hover:text-foreground"
								:aria-expanded="expandedRows.has(index)"
								aria-label="Toggle details"
								@click="toggleRow(index)"
							>
								<ChevronRight
									:class="[
										'h-3.5 w-3.5 transition-transform',
										expandedRows.has(index) && 'rotate-90',
									]"
									aria-hidden="true"
								/>
							</button>
						</td>
						<td
							v-for="column in node.columns"
							:key="column.key"
							:class="[
								'px-3 py-2',
								column.align ? columnAlignClass[column.align] : '',
							]"
						>
							<PluginView
								v-if="node.rows[index].cells[column.key]"
								:node="node.rows[index].cells[column.key]"
								:plugin-id="pluginId"
							/>
						</td>
					</tr>
					<tr
						v-if="node.rows[index].expand && expandedRows.has(index)"
						class="border-t border-border bg-muted/20"
					>
						<td
							:colspan="node.columns.length + (tableHasExpand ? 1 : 0)"
							class="px-3 py-2"
						>
							<div class="flex flex-col gap-1.5">
								<PluginView
									v-for="(child, childIndex) in node.rows[index].expand"
									:key="childIndex"
									:node="child"
									:plugin-id="pluginId"
								/>
							</div>
						</td>
					</tr>
				</template>
			</tbody>
		</table>
	</div>
	<ol v-else-if="node.type === 'timeline'" class="flex flex-col">
		<li
			v-for="(item, index) in node.items"
			:key="index"
			class="relative flex gap-3 pb-4 last:pb-0"
		>
			<span
				v-if="index < node.items.length - 1"
				class="absolute top-3 bottom-0 left-[5px] w-px bg-border"
				aria-hidden="true"
			/>
			<span
				:class="[
					'relative mt-1 h-[11px] w-[11px] shrink-0 rounded-full ring-2 ring-background',
					barClass(item.tone),
				]"
				aria-hidden="true"
			/>
			<div class="min-w-0 flex-1">
				<div class="text-2xs text-muted-foreground">{{ item.at }}</div>
				<div v-if="item.title" class="text-sm font-medium break-words">
					{{ item.title }}
				</div>
				<div class="text-sm break-words whitespace-pre-line">
					{{ item.text }}
				</div>
			</div>
		</li>
	</ol>
	<div v-else-if="node.type === 'messages'" class="flex flex-col gap-2">
		<div
			v-for="(item, index) in node.items"
			:key="index"
			:class="[
				'flex max-w-[85%] flex-col gap-0.5',
				item.side === 'right' ? 'items-end self-end' : 'items-start self-start',
			]"
		>
			<div class="text-2xs text-muted-foreground">
				{{ item.author }}<template v-if="item.at"> · {{ item.at }}</template>
			</div>
			<div
				:class="[
					'rounded-2xl px-3 py-1.5 text-sm break-words whitespace-pre-line',
					item.side === 'right'
						? 'rounded-br-sm bg-primary text-primary-foreground'
						: 'rounded-bl-sm bg-muted text-foreground',
				]"
			>
				{{ item.text }}
			</div>
		</div>
	</div>
	<svg
		v-else-if="node.type === 'sparkline'"
		:viewBox="`0 0 ${sparklineWidth} ${sparklineHeight}`"
		preserveAspectRatio="none"
		:class="['h-6 w-24', strokeClass(node.color)]"
		role="img"
		:aria-label="node.label || 'Sparkline'"
	>
		<polyline
			v-if="node.values.length > 0"
			:points="sparklinePointsText"
			fill="none"
			stroke="currentColor"
			stroke-width="1.5"
			stroke-linecap="round"
			stroke-linejoin="round"
			vector-effect="non-scaling-stroke"
		/>
	</svg>
	<div
		v-else-if="node.type === 'empty'"
		class="flex flex-col items-center gap-1 rounded-md border border-dashed border-border px-4 py-6 text-center"
	>
		<component
			:is="iconComponent(node.icon)"
			v-if="node.icon"
			class="h-6 w-6 text-muted-foreground"
			aria-hidden="true"
		/>
		<div class="text-sm font-medium">{{ node.title }}</div>
		<div
			v-if="node.text"
			class="text-sm break-words whitespace-pre-line text-muted-foreground"
		>
			{{ node.text }}
		</div>
	</div>
	<div
		v-else-if="node.type === 'skeleton'"
		class="flex flex-col gap-2"
		role="status"
		aria-label="Loading"
	>
		<template v-if="node.variant === 'card'">
			<Skeleton class="h-24 w-full" />
		</template>
		<template v-else-if="node.variant === 'table'">
			<Skeleton class="h-6 w-full" />
			<Skeleton v-for="n in node.lines" :key="n" class="h-8 w-full" />
		</template>
		<template v-else>
			<Skeleton
				v-for="n in node.lines"
				:key="n"
				:class="[
					'h-4',
					n === node.lines && node.lines > 1 ? 'w-2/3' : 'w-full',
				]"
			/>
		</template>
	</div>
	<AlertDialog v-else-if="node.type === 'button'" v-model:open="confirmOpen">
		<AppTooltip :content="buttonTooltip">
			<Button
				:variant="buttonVariantProp(node.variant)"
				:size="node.iconOnly ? 'icon' : buttonSizeProp(node.size)"
				:class="buttonClasses(node.variant, node.size, node.iconOnly)"
				:disabled="running || node.disabled || node.loading"
				:aria-label="node.iconOnly ? node.text : undefined"
				:aria-busy="node.loading ? 'true' : undefined"
				@click="handleButtonClick"
			>
				<Loader2 v-if="node.loading" class="animate-spin" />
				<component :is="iconComponent(node.icon)" v-else-if="node.icon" />
				<template v-if="!node.iconOnly">{{ node.text }}</template>
			</Button>
		</AppTooltip>
		<AlertDialogContent v-if="node.confirm">
			<AlertDialogHeader>
				<AlertDialogTitle
					>Plugin {{ pluginName }}: {{ node.text }}</AlertDialogTitle
				>
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
		class="text-2xs text-muted-foreground tabular-nums"
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
		class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1.5 text-sm"
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
	import AppTooltip from '@/components/general/AppTooltip.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { Skeleton } from '@/components/ui/skeleton';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuItem,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';
	import { toast } from '@/components/ui/toast';
	import { useCopyToClipboard } from '@/composable/useCopyToClipboard';
	import { usePluginTicker } from '@/composable/usePluginTicker';
	import { formatDueTime, formatTimeAgo } from '@/pluginSystem/relativeTime';
	import { pluginHost, pluginState } from '@/pluginSystem/state';
	import type {
		Color,
		IconName,
		MenuItem,
		Tone,
		UiNode,
	} from '@/pluginSystem/uiTree';
	import store from '@/store';
	import {
		ArrowDown,
		ArrowUp,
		ArrowUpDown,
		Check,
		ChevronRight,
		Copy,
		Loader2,
		MoreHorizontal,
	} from 'lucide-vue-next';
	import {
		computed,
		defineComponent,
		nextTick,
		onBeforeUnmount,
		ref,
		watch,
		type PropType,
	} from 'vue';
	import PluginMarkdown from './PluginMarkdown.vue';
	import {
		COLUMN_ALIGN_CLASS,
		COLUMN_WIDTH_CLASS,
		SPARKLINE_HEIGHT,
		SPARKLINE_WIDTH,
		nextSort,
		sortedIndexes,
		sparklinePoints,
		type SortState,
	} from './pluginData';
	import { PLUGIN_ICONS } from './pluginIcons';
	import {
		checkboxArgs,
		createDebouncer,
		inputArgs,
		nextInputText,
		sameControl,
		selectArgs,
	} from './pluginInputs';
	import {
		buttonClasses,
		buttonSizeProp,
		buttonVariantProp,
		cardInnerClasses,
		cardKeyActivates,
		cardOuterClasses,
		gridInnerClasses,
		gridTemplateStyle,
		isNestedInteractive,
		segmentedArgs,
		stackClasses,
		statClasses,
	} from './pluginViewClasses';

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

	const STROKES: Record<Color, string> = {
		gray: 'text-muted-foreground',
		green: 'text-emerald-500',
		yellow: 'text-amber-500',
		red: 'text-red-500',
		blue: 'text-sky-500',
		purple: 'text-purple-500',
		orange: 'text-orange-500',
	};

	const ROW_TINTS: Record<Color, string> = {
		gray: 'border-l-2 border-l-muted-foreground bg-muted/40',
		green: 'border-l-2 border-l-emerald-500 bg-emerald-500/10',
		yellow: 'border-l-2 border-l-amber-500 bg-amber-500/10',
		red: 'border-l-2 border-l-red-500 bg-red-500/10',
		blue: 'border-l-2 border-l-sky-500 bg-sky-500/10',
		purple: 'border-l-2 border-l-purple-500 bg-purple-500/10',
		orange: 'border-l-2 border-l-orange-500 bg-orange-500/10',
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
			AppTooltip,
			Button,
			Check,
			ChevronRight,
			Copy,
			Input,
			Skeleton,
			Loader2,
			DropdownMenu,
			DropdownMenuContent,
			DropdownMenuItem,
			DropdownMenuTrigger,
			MoreHorizontal,
			PluginMarkdown,
		},
		props: {
			node: { type: Object as PropType<UiNode>, required: true },
			pluginId: { type: String, required: true },
		},
		setup(props) {
			const running = ref(false);
			const confirmOpen = ref(false);
			const runCommand = async (command: string, rawArgs: unknown) => {
				running.value = true;
				try {
					// args can be a reactive proxy by the time it reaches here (the tree lives in a
					// reactive ref/record); postMessage to the plugin worker needs a plain, cloneable value.
					const args =
						rawArgs === undefined ? null : JSON.parse(JSON.stringify(rawArgs));
					await pluginHost()?.runCommand(props.pluginId, command, args);
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
			const run = () => {
				if (props.node.type !== 'button') return;
				confirmOpen.value = false;
				void runCommand(props.node.command, props.node.args);
			};
			const handleButtonClick = () => {
				if (props.node.type !== 'button') return;
				if (props.node.disabled || props.node.loading) return;
				if (props.node.confirm) confirmOpen.value = true;
				else void run();
			};
			const runSegmented = (value: string) => {
				const node = props.node;
				if (node.type !== 'segmented') return;
				if (!node.options.some((option) => option.value === value)) return;
				void runCommand(node.command, segmentedArgs(node.args, value));
			};
			const debouncer = createDebouncer();
			const inputText = ref(
				props.node.type === 'input' ? props.node.value : '',
			);
			const inputFocused = ref(false);
			watch(
				() => props.node,
				(node, previous) => {
					if (!sameControl(node, previous)) debouncer.cancel();
					inputText.value = nextInputText(
						node,
						previous,
						inputFocused.value,
						inputText.value,
					);
				},
			);
			onBeforeUnmount(() => debouncer.cancel());
			const sendInput = () => {
				const node = props.node;
				if (node.type !== 'input') return;
				void runCommand(node.command, inputArgs(node.args, inputText.value));
			};
			const onInputText = (typed: string | number) => {
				const node = props.node;
				if (node.type !== 'input') return;
				inputText.value = String(typed).slice(0, 500);
				if (node.mode === 'change') {
					debouncer.schedule(sendInput, node.debounce);
				}
			};
			const onInputEnter = (event: KeyboardEvent) => {
				const node = props.node;
				if (node.type !== 'input' || node.mode !== 'submit') return;
				if (event.isComposing) return;
				sendInput();
			};
			const onSelectChange = (event: Event) => {
				const node = props.node;
				if (node.type !== 'select') return;
				const args = selectArgs(
					node,
					(event.target as HTMLSelectElement).value,
				);
				if (args) void runCommand(node.command, args);
			};
			const onCheckboxChange = (event: Event) => {
				const node = props.node;
				if (node.type !== 'checkbox') return;
				void runCommand(
					node.command,
					checkboxArgs(node.args, (event.target as HTMLInputElement).checked),
				);
			};
			const buttonTooltip = computed(() =>
				props.node.type === 'button'
					? (props.node.tooltip ?? (props.node.iconOnly ? props.node.text : ''))
					: '',
			);
			const collapseOpen = ref(
				props.node.type === 'collapse' ? props.node.open : false,
			);
			const collapseShowAll = ref(false);
			const collapseChildren = computed(() => {
				const node = props.node;
				if (node.type !== 'collapse') return [];
				return node.visible && !collapseShowAll.value
					? node.children.slice(0, node.visible)
					: node.children;
			});
			const collapseToggleText = computed(() => {
				const node = props.node;
				if (node.type !== 'collapse' || !node.visible) return '';
				const hidden = node.children.length - node.visible;
				if (hidden <= 0) return '';
				return collapseShowAll.value ? 'Show less' : `${hidden} more`;
			});
			const selectedTabId = ref<string | null>(null);
			const activeTabId = computed(() => {
				const node = props.node;
				if (node.type !== 'tabs') return '';
				return node.tabs.some((tab) => tab.id === selectedTabId.value)
					? (selectedTabId.value as string)
					: node.selected;
			});
			const activeTabChildren = computed(() => {
				const node = props.node;
				if (node.type !== 'tabs') return [];
				return (
					node.tabs.find((tab) => tab.id === activeTabId.value)?.children ?? []
				);
			});
			const tableSort = ref<SortState | null>(null);
			const expandedRows = ref(new Set<number>());
			const tableRows = computed(() =>
				props.node.type === 'table' ? props.node.rows : [],
			);
			const tableOrder = computed(() =>
				sortedIndexes(tableRows.value, tableSort.value),
			);
			const tableHasExpand = computed(() =>
				tableRows.value.some((row) => row.expand !== undefined),
			);
			const toggleSort = (key: string) => {
				tableSort.value = nextSort(tableSort.value, key);
			};
			const ariaSort = (key: string) =>
				tableSort.value?.key !== key
					? 'none'
					: tableSort.value.dir === 'asc'
						? 'ascending'
						: 'descending';
			const sortIcon = (key: string) =>
				tableSort.value?.key !== key
					? ArrowUpDown
					: tableSort.value.dir === 'asc'
						? ArrowUp
						: ArrowDown;
			const toggleRow = (index: number) => {
				const next = new Set(expandedRows.value);
				if (!next.delete(index)) next.add(index);
				expandedRows.value = next;
			};
			const handleRowClick = (
				event: MouseEvent,
				row: { onClick?: { command: string; args?: unknown } },
			) => {
				if (!row.onClick) return;
				if (
					isNestedInteractive(
						event.target as Element | null,
						event.currentTarget as Element | null,
					)
				)
					return;
				void runCommand(row.onClick.command, row.onClick.args);
			};
			const handleRowKeydown = (
				event: KeyboardEvent,
				row: { onClick?: { command: string; args?: unknown } },
			) => {
				if (!row.onClick || event.target !== event.currentTarget) return;
				if (event.key === ' ') event.preventDefault();
				if (cardKeyActivates(event.key, true)) {
					void runCommand(row.onClick.command, row.onClick.args);
				}
			};
			const sparklinePointsText = computed(() =>
				props.node.type === 'sparkline'
					? sparklinePoints(props.node.values)
					: '',
			);
			const runNodeCommand = (node: UiNode) => {
				if ((node.type === 'badge' || node.type === 'stat') && node.command) {
					void runCommand(node.command, node.args);
				}
			};
			const handleCardClick = (event: MouseEvent) => {
				if (props.node.type !== 'card' || !props.node.onClick) return;
				if (
					isNestedInteractive(
						event.target as Element | null,
						event.currentTarget as Element | null,
					)
				)
					return;
				void runCommand(props.node.onClick.command, props.node.onClick.args);
			};
			const handleCardKeydown = (event: KeyboardEvent) => {
				if (props.node.type !== 'card' || !props.node.onClick) return;
				if (event.target !== event.currentTarget) return;
				if (event.key === ' ') event.preventDefault();
				if (cardKeyActivates(event.key, true)) {
					void runCommand(props.node.onClick.command, props.node.onClick.args);
				}
			};
			const menuConfirmOpen = ref(false);
			const menuConfirmItem = ref<MenuItem | null>(null);
			const handleMenuSelect = (item: MenuItem) => {
				if (item.confirm) {
					void nextTick(() => {
						menuConfirmItem.value = item;
						menuConfirmOpen.value = true;
					});
				} else {
					void runCommand(item.command, item.args);
				}
			};
			const runMenuConfirm = () => {
				const item = menuConfirmItem.value;
				if (!item) return;
				menuConfirmOpen.value = false;
				void runCommand(item.command, item.args);
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
				if (props.node.type === 'copyable' || props.node.type === 'code')
					void copy(props.node.text);
			};
			const isCopied = computed(
				() =>
					(props.node.type === 'copyable' || props.node.type === 'code') &&
					copiedText.value === props.node.text,
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
				() =>
					pluginState.plugins[props.pluginId]?.manifest.name ?? props.pluginId,
			);
			return {
				running,
				confirmOpen,
				run,
				runSegmented,
				inputText,
				inputFocused,
				onInputText,
				onInputEnter,
				onSelectChange,
				onCheckboxChange,
				buttonTooltip,
				collapseOpen,
				collapseShowAll,
				collapseChildren,
				collapseToggleText,
				selectedTabId,
				activeTabId,
				activeTabChildren,
				iconComponent: (name: IconName) => PLUGIN_ICONS[name],
				buttonClasses,
				handleButtonClick,
				tableOrder,
				tableHasExpand,
				expandedRows,
				toggleSort,
				ariaSort,
				sortIcon,
				toggleRow,
				handleRowClick,
				handleRowKeydown,
				sparklinePointsText,
				sparklineWidth: SPARKLINE_WIDTH,
				sparklineHeight: SPARKLINE_HEIGHT,
				columnAlignClass: COLUMN_ALIGN_CLASS,
				columnWidthClass: COLUMN_WIDTH_CLASS,
				strokeClass: (color: Color) => STROKES[color],
				rowTintClass: (color: Color) => ROW_TINTS[color],
				runNodeCommand,
				handleCardClick,
				handleCardKeydown,
				menuConfirmOpen,
				menuConfirmItem,
				handleMenuSelect,
				runMenuConfirm,
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
				stackClasses,
				statClasses,
				cardOuterClasses,
				cardInnerClasses,
				gridInnerClasses,
				gridTemplateStyle,
				buttonVariantProp,
				buttonSizeProp,
			};
		},
	});
</script>
