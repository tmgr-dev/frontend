<template>
	<div>
		<div class="editorjs" ref="editorRef"></div>
	</div>
</template>

<script setup>
	import {
		createEditorOperationQueue,
		createSerialEditor,
	} from '@/utils/serialEditor';
	import { normalizeBlocks, sameBlocks } from '@/utils/editor';
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
			!event.target.contains(
				selection.getRangeAt(0).commonAncestorContainer,
			)
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

	/* Improve readability in dark mode */
	.dark .ce-block--selected .ce-block__content {
		background-color: rgba(255, 255, 255, 0.1);
	}

	/* Fix text selection in dark mode - ensure text remains readable */
	.dark .editorjs ::selection {
		background-color: rgba(59, 130, 246, 0.5);
		color: #ffffff;
	}

	.dark .editorjs ::-moz-selection {
		background-color: rgba(59, 130, 246, 0.5);
		color: #ffffff;
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
</style>
