<script setup lang="ts">
	import '@/assets/styles/md-editor.css';
	import { EditorView } from '@codemirror/view';
	import { MdEditor, config } from 'md-editor-v3';
	import { onBeforeUnmount } from 'vue';
	import store from '../store';

	config({
		editorExtensions: { screenfull: { instance: { isEnabled: false } as any } },
	});

	const editorId = `md-editor-${Math.random().toString(36).slice(2)}`;
	const value = defineModel();
	onBeforeUnmount(() => {
		const dom = document.querySelector<HTMLElement>(
			`#${editorId} .cm-editor`,
		);
		if (dom) EditorView.findFromDOM(dom)?.destroy();
	});
</script>

<template>
	<MdEditor
		:editor-id="editorId"
		v-model="value"
		:preview="false"
		language="en-US"
		placeholder="Add description..."
		:theme="store.getters.isDarkTheme ? 'dark' : 'light'"
		:toolbars-exclude="[
			'sub',
			'sup',
			'mermaid',
			'katex',
			'revoke',
			'next',
			'save',
			'prettier',
			'fullscreen',
			'preview',
			'previewOnly',
			'htmlPreview',
			'catalog',
			'github',
		]"
		:footers="[]"
	/>
</template>

<style>
	/* CodeMirror's own theme rule is more specific than any selector we can target. */
	.md-editor .cm-editor .cm-selectionBackground {
		background: var(--selection-bg) !important;
	}

	.md-editor-dark:not(.md-editor-fullscreen) {
		--md-bk-color: transparent;
	}

	.md-editor:not(.md-editor-dark, .md-editor-fullscreen) {
		--md-bk-color: var(--bg-sunken);
		--md-bk-color-outstand: var(--bg-sunken);
	}

	.md-editor:not(.md-editor-dark, .md-editor-fullscreen) .md-editor-content,
	.md-editor:not(.md-editor-dark, .md-editor-fullscreen) .md-editor-input,
	.md-editor:not(.md-editor-dark, .md-editor-fullscreen)
		.md-editor-preview-wrapper,
	.md-editor:not(.md-editor-dark, .md-editor-fullscreen) .md-editor-preview {
		background-color: var(--bg-sunken) !important;
	}
</style>
