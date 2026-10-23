<template>
	<div>
		<div class="editorjs" ref="editorRef"></div>
	</div>
</template>

<script setup>
	import { normalizeBlocks, sameBlocks } from '@/utils/editor';
	import {
		createEditorOperationQueue,
		createSerialEditor,
	} from '@/utils/serialEditor';
	import { escapeCarriedHtml } from '@/utils/titleOverflow';
	import CodeTool from '@bomdi/codebox';
	import Delimiter from '@editorjs/delimiter';
	import EditorJS from '@editorjs/editorjs';
	import Embed from '@editorjs/embed';
	import Header from '@editorjs/header';
	import InlineCode from '@editorjs/inline-code';
	import List from '@editorjs/list';
	import Marker from '@editorjs/marker';
	import Quote from '@editorjs/quote';
	import Raw from '@editorjs/raw';
	import Table from '@editorjs/table';
	import Warning from '@editorjs/warning';
	import DragDrop from 'editorjs-drag-drop';
	import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';

	const editorRef = ref(null);
	const props = defineProps(['modelValue', 'placeholder']);
	const emit = defineEmits(['update:modelValue']);
	let editor;
	let updatingModel = false;
	let isEditorReady = false;
	let dragDropInitialized = false;
	let disposed = false;
	let rendering = false;
	let lastPublished;
	let lastRequested;
	const runOperation = createEditorOperationQueue();
	const serial = createSerialEditor(
		() =>
			runOperation(async () => {
				await editor.isReady;
				return editor.save();
			}),
		(output) => {
			if (
				typeof props.modelValue !== 'string' &&
				sameBlocks(output.blocks, props.modelValue)
			)
				return;
			lastPublished = output;
			if (!disposed) emit('update:modelValue', output);
		},
	);
	defineExpose({
		flush: () => serial.changed(),
		replace: (value) => renderModel(value),
		hasPending: serial.hasPending,
		// Awaiting readiness first queues the initial render (onReady) ahead of this
		// insert; render() would otherwise erase the new block.
		prependParagraph: async (text) => {
			await editor.isReady;
			return runOperation(async () => {
				if (disposed) return false;
				// renderFromHTML resolves before Editor.js has pasted the legacy HTML
				// into the editor (async pattern processing); inserting now would be
				// replacing the still-empty block and the HTML would be lost.
				if (typeof lastRequested === 'string' && lastRequested.trim())
					for (
						let i = 0;
						i < 50 &&
						!editor.blocks.getBlockByIndex(0)?.holder.textContent.trim();
						i++
					)
						await new Promise((resolve) => setTimeout(resolve, 20));
				const carried = typeof text === 'function' ? text() : text;
				const first = editor.blocks.getBlockByIndex(0);
				const replaceEmpty =
					editor.blocks.getBlocksCount() === 1 &&
					first?.name === 'paragraph' &&
					!first.holder.textContent.trim();
				editor.blocks.insert(
					'paragraph',
					{ text: escapeCarriedHtml(carried) },
					undefined,
					0,
					false,
					replaceEmpty,
				);
				editor.caret.setToBlock(0, 'end');
				return true;
			});
		},
	});
	function renderModel(value) {
		lastRequested = value;
		return runOperation(async () => {
			await editor.isReady;
			rendering = true;
			try {
				if (typeof value === 'string')
					await editor.blocks.renderFromHTML(value);
				else await editor.render(normalizeBlocks(value));
			} finally {
				rendering = false;
			}
		});
	}

	function initDragDrop() {
		if (dragDropInitialized || !editor || !editor.configuration?.holder) {
			return;
		}
		try {
			new DragDrop(editor);
			dragDropInitialized = true;
		} catch (e) {
			console.warn('DragDrop initialization failed:', e);
		}
	}

	async function modelToView() {
		if (
			disposed ||
			!isEditorReady ||
			!editor ||
			props.modelValue === lastPublished ||
			props.modelValue === lastRequested
		) {
			return;
		}
		initDragDrop();
		if (!props.modelValue) {
			return;
		}
		if (
			serial.hasPending() ||
			editorRef.value?.contains(document.activeElement)
		)
			return;
		await renderModel(props.modelValue);
	}

	// view -> model
	function viewToModel() {
		if (disposed || rendering) return;
		updatingModel = true;
		void serial
			.changed()
			.catch((error) => console.error('Editor serialization failed', error))
			.finally(async () => {
				await nextTick();
				updatingModel = false;
			});
	}

	// Editor.js binds a new block's keydown handler in requestIdleCallback, so an Enter right
	// after the block appears gets the native line break: https://github.com/codex-team/editor.js/issues/2470
	function enterBeforeBlockEventsBound(event) {
		if (
			event.keyCode !== 13 ||
			event.shiftKey ||
			event.isComposing ||
			event.defaultPrevented ||
			!isEditorReady ||
			editorRef.value.querySelector('.ce-popover--opened')
		)
			return;
		const index = editor.blocks.getCurrentBlockIndex();
		const block = editor.blocks.getBlockByIndex(index);
		const selection = window.getSelection();
		if (
			index < 0 ||
			!['paragraph', 'header'].includes(block?.name) ||
			!block.holder.contains(event.target) ||
			!selection.rangeCount ||
			!event.target.contains(selection.getRangeAt(0).commonAncestorContainer)
		)
			return;
		event.preventDefault();
		const caret = selection.getRangeAt(0);
		caret.deleteContents();
		const before = document.createRange();
		before.selectNodeContents(event.target);
		before.setEnd(caret.startContainer, caret.startOffset);
		const after = document.createRange();
		after.selectNodeContents(event.target);
		after.setStart(caret.startContainer, caret.startOffset);
		if (!before.toString()) {
			editor.blocks.insert('paragraph', {}, undefined, index, false);
			return;
		}
		const rest = document.createElement('div');
		if (after.toString()) rest.append(after.extractContents());
		editor.blocks.insert(
			'paragraph',
			{ text: rest.innerHTML },
			undefined,
			index + 1,
		);
		editor.caret.setToBlock(index + 1, 'start');
	}

	watch(
		() => props.modelValue,
		() => {
			if (!updatingModel) {
				modelToView();
			}
		},
	);

	onMounted(() => {
		editorRef.value.addEventListener('keydown', enterBeforeBlockEventsBound);
		const initialData = props.modelValue;
		lastRequested = initialData;
		editor = new EditorJS({
			holder: editorRef.value,
			placeholder: props.placeholder,
			inlineToolbar: ['bold', 'italic', 'link'],
			tools: {
				header: {
					class: Header,
					config: {
						placeholder: 'Enter a header',
						levels: [1, 2, 3],
						defaultLevel: 1,
					},
				},
				list: {
					class: List,
					inlineToolbar: true,
					config: {
						defaultStyle: 'unordered',
					},
				},
				quote: {
					class: Quote,
					inlineToolbar: true,
					shortcut: 'CMD+SHIFT+O',
					config: {
						quotePlaceholder: 'Enter a quote',
						captionPlaceholder: "Quote's author",
					},
				},
				warning: {
					class: Warning,
					inlineToolbar: true,
					shortcut: 'CMD+SHIFT+W',
					config: {
						titlePlaceholder: 'Title',
						messagePlaceholder: 'Message',
					},
				},
				marker: {
					class: Marker,
					shortcut: 'CMD+SHIFT+M',
				},
				codebox: {
					// The package publishes a CommonJS namespace wrapping its default class.
					class: CodeTool.default ?? CodeTool,
					shortcut: 'CMD+SHIFT+C',
					config: {
						themeURL:
							'https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@9.18.1/build/styles/dracula.min.css', // Optional
						themeName: 'atom-one-dark', // Optional
						useDefaultTheme: 'light', // Optional. This also determines the background color of the language select drop-down
					},
				},
				delimiter: Delimiter,
				inlineCode: {
					class: InlineCode,
					shortcut: 'CMD+SHIFT+C',
				},
				table: {
					class: Table,
					inlineToolbar: true,
					config: {
						rows: 2,
						cols: 3,
					},
				},
				raw: Raw,
				// image: {
				// 	class: ImageTool,
				// 	config: {
				// 		uploader: {
				// 			uploadByFile: uploadImage
				// 		},
				// 		captionPlaceholder: 'Image caption'
				// 	}
				// },
				embed: {
					class: Embed,
					config: {
						services: {
							youtube: true,
							codesandbox: true,
							codepen: true,
							figma: true,
						},
					},
				},
			},
			minHeight: 'auto',
			data:
				typeof initialData === 'string' || !initialData
					? undefined
					: normalizeBlocks(initialData),
			onReady: () => {
				isEditorReady = true;
				initDragDrop();
				if (typeof initialData === 'string') void renderModel(props.modelValue);
				else void modelToView();
			},
			onChange: viewToModel,
		});
	});

	onBeforeUnmount(() => {
		editorRef.value?.removeEventListener(
			'keydown',
			enterBeforeBlockEventsBound,
		);
		disposed = true;
		isEditorReady = false;
		dragDropInitialized = false;
		if (editor) {
			void serial
				.flush()
				.catch((error) => console.error('Editor final save failed', error))
				.then(() =>
					runOperation(() => editor.isReady.then(() => editor.destroy())),
				)
				.catch((error) => console.error('Editor cleanup failed', error));
		}
	});
</script>

<style>
	/* Fix for modal windows with block editor content */
	.editorjs {
		/* Ensure the editor itself is properly sized */
		height: 100%;
		min-height: 200px;
		max-height: 100%;
		overflow-y: visible;
		margin-left: 0px;
		padding-left: 0px !important;
		width: calc(100% - 30px);
		position: relative;
	}

	.ce-block__content {
		word-break: break-word;
		overflow-wrap: break-word;
		max-width: 100%; /* Ensure content doesn't overflow */
	}

	/* Make list items display properly in modal */
	.cdx-list__item {
		word-wrap: break-word;
		overflow-wrap: break-word;
		padding-right: 8px;
		margin-left: 30px; /* Give space for the bullet/number */
	}

	/* Limit bullet point text length in modal */
	.cdx-list {
		max-width: 100%;
		padding-right: 16px;
	}

	/* Hang the counter left of the item like a native <ol> marker, so its width doesn't shift the text */
	.cdx-list-ordered .cdx-list__item {
		position: relative;
	}

	.cdx-list-ordered .cdx-list__item::before {
		position: absolute;
		right: 100%;
	}

	.dark .cdx-list {
		--checkbox-background: transparent;
	}

	.codex-editor .ce-block--selected .ce-block__content {
		background-color: var(--bg-active);
	}

	.editorjs .codex-editor ::selection {
		background-color: var(--selection-bg);
		color: var(--fg);
	}

	.editorjs .codex-editor ::-moz-selection {
		background-color: var(--selection-bg);
		color: var(--fg);
	}

	/* Ensure content is visible within the modal */
	.ce-block__content {
		max-width: 100% !important; /* Override EditorJS default */
		margin: 0 5px !important; /* Provide slight margin but not too much */
		padding: 0 5px;
	}

	/* Fix block width to prevent overflow */
	.ce-block {
		max-width: 100% !important;
	}

	/* Fix for toolbar positioning in modal */
	.ce-toolbar {
		position: absolute !important;
		z-index: 100 !important;
		margin-left: -30px !important; /* Align with the content edge */
	}

	/* Fix for plus button placement */
	.ce-toolbar__plus {
		position: absolute !important;
		left: -25px !important;
		transform: none !important;
		opacity: 0.8 !important;
		top: 5px !important;
	}

	/* Ensure the plus button is visible on hover */
	.ce-toolbar__plus:hover {
		opacity: 1 !important;
	}

	/* Ensure the actions are properly aligned */
	.ce-toolbar__actions {
		right: 0 !important;
		transform: none !important;
	}

	/* Fix for text overflow with long words */
	.ce-paragraph {
		word-break: break-word;
		overflow-wrap: break-word;
		width: 100% !important;
	}

	/* Ensure proper spacing in the toolbar */
	.ce-toolbar__content {
		max-width: 100% !important;
		position: relative !important;
	}

	/* Ensure long words don't cause horizontal scrolling */
	.ce-block__content *,
	.cdx-block,
	.ce-paragraph {
		max-width: 100% !important;
		overflow-wrap: break-word !important;
		word-wrap: break-word !important;
	}

	/* Ensure proper block alignment */
	.codex-editor {
		position: relative !important;
		padding-left: 30px !important; /* Ensure there's space for the plus button */
		margin-left: -30px !important; /* Offset the padding */
		box-sizing: border-box !important;
	}

	/* Additional fix for the popover alignment */
	.ce-popover {
		margin-left: 0 !important;
	}

	/* Long strings like in your screenshot example */
	.ce-paragraph[data-placeholder]:empty::before {
		max-width: 90%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	/* Fix for extremely long strings without spaces */
	.ce-paragraph {
		overflow-wrap: break-word;
		word-wrap: break-word;
		-ms-word-break: break-all;
		word-break: break-word;
		-ms-hyphens: auto;
		-moz-hyphens: auto;
		-webkit-hyphens: auto;
		hyphens: auto;
	}
	/* Editor.js floating UI reads these custom properties; point them at the app tokens. */
	.codex-editor .ce-popover,
	.codex-editor .ce-inline-toolbar {
		--color-border: var(--line-color);
		--color-background: var(--bg-raised);
		--color-text-primary: var(--fg);
		--color-text-secondary: var(--fg-muted);
		--color-border-icon: var(--line-color);
		--color-border-icon-disabled: var(--line-color);
		--color-text-icon-active: var(--brand-color);
		--color-background-icon-active: color-mix(
			in srgb,
			var(--brand-color) 14%,
			transparent
		);
		--color-background-item-focus: color-mix(
			in srgb,
			var(--brand-color) 10%,
			transparent
		);
		--color-shadow-item-focus: color-mix(
			in srgb,
			var(--brand-color) 20%,
			transparent
		);
		--color-background-item-hover: var(--bg-hover);
	}

	.codex-editor .ce-popover__container {
		box-shadow: var(--shadow-md);
	}

	.codex-editor .ce-popover__nothing-found-message,
	.codex-editor .cdx-search-field__icon svg,
	.codex-editor .cdx-search-field__input::placeholder,
	.codex-editor .ce-inline-tool-input::placeholder {
		color: var(--fg-subtle);
	}

	.codex-editor .cdx-search-field__input,
	.codex-editor .ce-inline-tool-input {
		color: var(--fg);
	}

	.codex-editor .cdx-search-field,
	.codex-editor .ce-inline-tool-input {
		background: var(--bg-sunken);
		border-color: var(--line-color);
	}

	.codex-editor .ce-inline-toolbar__dropdown {
		border-right-color: var(--line-color);
	}

	.codex-editor .ce-inline-toolbar__dropdown:hover,
	.codex-editor .ce-inline-tool:hover,
	.codex-editor .cdx-settings-button:hover,
	.codex-editor .ce-toolbar__plus:hover,
	.codex-editor .ce-toolbar__settings-btn:hover {
		background-color: var(--bg-hover);
	}

	.codex-editor .ce-toolbar__plus--active,
	.codex-editor .ce-toolbar__settings-btn--active {
		background-color: var(--bg-active);
	}

	.codex-editor .ce-toolbar__plus,
	.codex-editor .ce-toolbar__settings-btn {
		color: var(--fg-muted);
	}

	.codex-editor .cdx-settings-button--active {
		color: var(--brand-color);
	}

	.codex-editor .cdx-settings-button--focused {
		background: color-mix(
			in srgb,
			var(--brand-color) 10%,
			transparent
		) !important;
	}
	/* Editor.js plugin content: library defaults are light-palette, point them at the tokens. */
	.codex-editor .inline-code {
		background: var(--bg-active);
		color: var(--fg);
	}

	.codex-editor .cdx-input {
		border-color: var(--line-strong-color);
		box-shadow: none;
	}

	.codex-editor .cdx-list {
		--color-border: var(--line-strong-color);
		--color-bg-checked: var(--brand-color);
		--color-bg-checked-hover: var(--brand-hover-color);
		--color-tick: hsl(var(--primary-foreground));
	}

	.codex-editor .cdx-list-start-with-field {
		background: var(--bg-sunken);
		border-color: var(--line-color);
	}

	.codex-editor .cdx-list-start-with-field__input {
		color: var(--fg);
	}

	.codex-editor .tc-wrap,
	.codex-editor .tc-popover {
		--color-border: var(--line-strong-color);
		--color-background: var(--bg-sunken);
		--color-background-hover: var(--bg-hover);
		--color-text-secondary: var(--fg-muted);
	}

	.codex-editor .tc-popover {
		--color-border: var(--line-color);
		--color-background: var(--bg-raised);
	}

	.codex-editor .tc-toolbox {
		--toggler-dots-color: var(--fg-subtle);
		--toggler-dots-color-hovered: var(--fg);
	}

	.dark .codex-editor .cdx-warning::before {
		filter: invert(1);
	}

	.dark .codex-editor .codeBoxHolder .light {
		background-color: var(--bg-sunken);
		color: var(--fg);
	}
</style>
