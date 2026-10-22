<template>
	<div class="blockmd-editor" ref="rootRef"></div>
</template>

<script setup lang="ts">
	import { Crepe } from '@milkdown/crepe';
	import '@milkdown/crepe/theme/common/style.css';
	import darkTheme from '@milkdown/crepe/theme/frame-dark.css?inline';
	import lightTheme from '@milkdown/crepe/theme/frame.css?inline';
	import { editorViewCtx } from '@milkdown/kit/core';
	import { TextSelection } from '@milkdown/kit/prose/state';
	import { replaceAll } from '@milkdown/kit/utils';
	import { onBeforeUnmount, onMounted, ref, watch } from 'vue';
	import store from '../store';

	interface Props {
		modelValue: string | null;
		placeholder?: string;
	}

	const props = defineProps<Props>();
	const emit = defineEmits<{ 'update:modelValue': [value: string] }>();

	const rootRef = ref<HTMLElement | null>(null);
	let crepe: Crepe | null = null;
	let lastMarkdown = '';
	let hasUserEdits = false;
	let ready = false;
	let markReady: () => void = () => {};
	const whenReady = new Promise<void>((resolve) => (markReady = resolve));

	// markdownUpdated is debounced (200ms) and cancelled on destroy.
	defineExpose({
		flush: () => {
			if (!ready) return;
			const markdown = crepe?.getMarkdown();
			if (markdown === undefined || markdown === lastMarkdown) return;
			lastMarkdown = markdown;
			hasUserEdits = true;
			emit('update:modelValue', markdown);
		},
		prependParagraph: async (text: string | (() => string)) => {
			await whenReady;
			if (!crepe || !ready) return false;
			const carried = typeof text === 'function' ? text() : text;
			crepe.editor.action((ctx) => {
				const view = ctx.get(editorViewCtx);
				const { doc, schema } = view.state;
				const content = carried
					.split('\n')
					.flatMap((line, i) => [
						...(i && schema.nodes.hardbreak
							? [schema.nodes.hardbreak.create()]
							: []),
						...(line ? [schema.text(line)] : []),
					]);
				const paragraph = schema.nodes.paragraph.create(null, content);
				const empty =
					doc.childCount === 1 &&
					doc.firstChild!.isTextblock &&
					doc.firstChild!.content.size === 0;
				const tr = empty
					? view.state.tr.replaceWith(0, doc.content.size, paragraph)
					: view.state.tr.insert(0, paragraph);
				tr.setSelection(TextSelection.create(tr.doc, paragraph.nodeSize - 1));
				view.dispatch(tr.scrollIntoView());
				view.focus();
			});
			return true;
		},
	});

	// frame.css and frame-dark.css both target `.milkdown`, so only one may be
	// mounted at a time; swap the <style> element when the color scheme changes.
	const themeStyle = document.createElement('style');
	themeStyle.dataset.blockmdTheme = '';
	const applyTheme = () => {
		themeStyle.textContent = store.getters.isDarkTheme ? darkTheme : lightTheme;
	};

	onMounted(async () => {
		applyTheme();
		document.head.appendChild(themeStyle);

		lastMarkdown = props.modelValue ?? '';
		crepe = new Crepe({
			root: rootRef.value!,
			defaultValue: lastMarkdown,
			features: {
				[Crepe.Feature.CodeMirror]: false,
				[Crepe.Feature.Latex]: false,
				[Crepe.Feature.ImageBlock]: false,
			},
			featureConfigs: {
				// Handle (add / drag) on the right, like Editor.js: on the left it is
				// cut off by the task panel edge.
				[Crepe.Feature.BlockEdit]: {
					blockHandle: {
						getPlacement: () => 'right-start',
						// keep it inside the canvas' right padding
						getOffset: () => ({ mainAxis: 8 }),
					},
				},
				[Crepe.Feature.Placeholder]: {
					text: props.placeholder ?? 'Add description...',
					mode: 'block',
				},
			},
		});
		crepe.on((listener) => {
			listener.markdownUpdated((_ctx, markdown) => {
				if (markdown === lastMarkdown) return;
				lastMarkdown = markdown;
				hasUserEdits = true;
				emit('update:modelValue', markdown);
			});
		});
		try {
			await crepe.create();
			ready = true;
		} finally {
			markReady();
		}
	});

	// model -> view (external change, e.g. task reload)
	watch(
		() => props.modelValue,
		(value) => {
			const next = value ?? '';
			if (!crepe || next === lastMarkdown) return;
			// A stale value arriving mid-typing (e.g. autosave response) must not
			// clobber the document; push the current markdown back to the parent instead.
			// Before any user edit the incoming value is authoritative (initial task load).
			if (hasUserEdits && rootRef.value?.contains(document.activeElement)) {
				emit('update:modelValue', lastMarkdown);
				return;
			}
			lastMarkdown = next;
			crepe.editor.action(replaceAll(next));
		},
	);

	watch(() => store.getters.isDarkTheme, applyTheme);

	onBeforeUnmount(() => {
		markReady();
		crepe?.destroy();
		crepe = null;
		themeStyle.remove();
	});
</script>

<style>
	/* Only the editor canvas is transparent; --crepe-color-surface must stay
	   intact because Crepe paints the slash menu, toolbar and tooltips with it. */
	.blockmd-editor .milkdown {
		--crepe-color-background: transparent;
		--crepe-color-selected: var(--selection-bg);
		background: transparent;
	}

	/* Keep the slash menu short enough to fit below the caret inside the task panel */
	.blockmd-editor .milkdown-slash-menu .menu-groups {
		max-height: 240px;
		overflow-y: auto;
	}

	.blockmd-editor .milkdown .ProseMirror {
		padding: 8px 80px 16px 16px;
		min-height: 240px;
	}
</style>
