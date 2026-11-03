<template>
	<template v-for="(item, index) in nodes" :key="index">
		<strong v-if="item.type === 'strong'" class="font-semibold">
			<PluginMarkdownInline :nodes="item.children" :plugin-id="pluginId" />
		</strong>
		<em v-else-if="item.type === 'em'">
			<PluginMarkdownInline :nodes="item.children" :plugin-id="pluginId" />
		</em>
		<del v-else-if="item.type === 'del'" class="text-muted-foreground">
			<PluginMarkdownInline :nodes="item.children" :plugin-id="pluginId" />
		</del>
		<code
			v-else-if="item.type === 'codespan'"
			class="rounded bg-muted px-1 py-0.5 font-mono text-[0.85em] break-words text-foreground"
			>{{ item.text }}</code
		>
		<br v-else-if="item.type === 'br'" />
		<button
			v-else-if="item.type === 'link'"
			type="button"
			class="inline text-left text-primary hover:underline"
			@click="open(item.url)"
		>
			<PluginMarkdownInline :nodes="item.children" :plugin-id="pluginId" />
			<span class="ml-1 text-2xs text-muted-foreground">({{ item.host }})</span>
		</button>
		<template v-else-if="item.type === 'text'">{{ item.text }}</template>
	</template>
</template>

<script lang="ts">
	import { pluginHost } from '@/pluginSystem/state';
	import type { MdInline } from '@/pluginSystem/uiTree';
	import { defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'PluginMarkdownInline',
		props: {
			nodes: { type: Array as PropType<MdInline[]>, required: true },
			pluginId: { type: String, required: true },
		},
		setup(props) {
			return {
				open: (url: string) => pluginHost()?.openLink(props.pluginId, url),
			};
		},
	});
</script>
