<template>
	<MdPreview
		:id="editorId"
		:model-value="content"
		language="en-US"
		:theme="store.getters.isDarkTheme ? 'dark' : 'light'"
		class="description-markdown"
		no-echarts
		:code-foldable="false"
	/>
</template>

<script lang="ts">
	import '@/assets/styles/md-editor.css';
	import '@/utils/mdEditorConfig';
	import store from '@/store';
	import { MdPreview } from 'md-editor-v3';
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'DescriptionMarkdown',
		components: { MdPreview },
		props: {
			content: { type: String, default: '' },
		},
		setup() {
			return {
				store,
				editorId: `md-preview-${Math.random().toString(36).slice(2)}`,
			};
		},
	});
</script>

<style>
	.md-editor.description-markdown {
		--md-bk-color: transparent;
		height: auto;
		min-height: 0;
		border: 0;
		background: transparent;
	}

	/* Tailwind preflight resets list-style to none. */
	.description-markdown .md-editor-preview ul {
		list-style: disc;
	}

	.description-markdown .md-editor-preview ol {
		list-style: decimal;
	}
</style>
