<template>
	<div class="blockmd-editor" ref="rootRef"></div>
</template>

<script setup lang="ts">
	import { onBeforeUnmount, onMounted, ref, watch } from 'vue';
	import { Crepe } from '@milkdown/crepe';
	import { replaceAll } from '@milkdown/kit/utils';
	import '@milkdown/crepe/theme/common/style.css';
	import lightTheme from '@milkdown/crepe/theme/frame.css?inline';
	import darkTheme from '@milkdown/crepe/theme/frame-dark.css?inline';
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

	// frame.css and frame-dark.css both target `.milkdown`, so only one may be
	// mounted at a time; swap the <style> element when the color scheme changes.
	const themeStyle = document.createElement('style');
	themeStyle.dataset.blockmdTheme = '';
	const applyTheme = () => {
		themeStyle.textContent =
			store.state.colorScheme === 'default' ? lightTheme : darkTheme;
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
		await crepe.create();
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

	watch(() => store.state.colorScheme, applyTheme);

	onBeforeUnmount(() => {
		crepe?.destroy();
		crepe = null;
		themeStyle.remove();
	});
</script>

<style>
	.blockmd-editor .milkdown {
		--crepe-color-background: transparent;
		--crepe-color-surface: transparent;
	}

	.blockmd-editor .milkdown .ProseMirror {
		padding: 8px 16px 16px 40px;
		min-height: 240px;
	}
</style>
