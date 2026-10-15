import { imageFilesOf } from '@/utils/pages/files';
import { parseTmgrUrl } from '@/utils/pages/tmgrLinks';
import { syntaxHighlighting } from '@codemirror/language';
import { languages } from '@codemirror/language-data';
import { shift, size } from '@floating-ui/dom';
import { classHighlighter } from '@lezer/highlight';
import { Crepe } from '@milkdown/crepe';
import {
	commandsCtx,
	editorViewCtx,
	remarkStringifyOptionsCtx,
	schemaCtx,
} from '@milkdown/kit/core';
import type { Ctx } from '@milkdown/kit/ctx';
import { uploadConfig } from '@milkdown/kit/plugin/upload';
import {
	clearTextInCurrentBlockCommand,
	linkAttr,
} from '@milkdown/kit/preset/commonmark';
import { remarkGFMPlugin } from '@milkdown/kit/preset/gfm';
import type { Node, Schema } from '@milkdown/kit/prose/model';
import { Plugin, PluginKey } from '@milkdown/kit/prose/state';
import { DecorationSet } from '@milkdown/kit/prose/view';
import { $prose } from '@milkdown/kit/utils';
import type { TmgrDirectory } from '../useTmgrDirectory';
import { icons } from './icons';

export interface PageCrepeOptions {
	root: HTMLElement;
	markdown: string;
	placeholder: string;
	directory: TmgrDirectory;
	uploadFile?: (file: File) => Promise<number>;
	onUploadError: (file: File) => void;
	onMakeTask: (text: string) => void;
}

const item = (label: string, icon: string) => ({ label, icon });

const pickKey = new PluginKey<DecorationSet>('tmgr-image-pick');

const pickPlaceholders = $prose(
	(ctx) =>
		new Plugin<DecorationSet>({
			key: pickKey,
			state: {
				init: () => DecorationSet.empty,
				apply(tr, set) {
					const next = set.map(tr.mapping, tr.doc);
					const meta = tr.getMeta(pickKey);
					if (meta?.add) {
						const { uploadWidgetFactory } = ctx.get(uploadConfig.key);
						return next.add(tr.doc, [
							uploadWidgetFactory(meta.add.pos, { id: meta.add.id }),
						]);
					}
					if (meta?.remove) {
						return next.remove(
							next.find(
								undefined,
								undefined,
								(spec) => spec.id === meta.remove,
							),
						);
					}
					return next;
				},
			},
			props: { decorations: (state) => pickKey.getState(state) },
		}),
);

export const createPageCrepe = (options: PageCrepeOptions): Crepe => {
	const uploadImages = async (
		list: ArrayLike<File>,
		schema: Schema,
	): Promise<Node[]> => {
		const upload = options.uploadFile;
		if (!upload) return [];
		const nodes = [];
		for (const file of imageFilesOf(list)) {
			try {
				const id = await upload(file);
				const node = schema.nodes.image.createAndFill({
					src: `tmgr://file/${id}`,
					alt: file.name,
				});
				if (node) nodes.push(node);
			} catch {
				options.onUploadError(file);
			}
		}
		return nodes;
	};

	const pickImages = (ctx: Ctx) => {
		ctx.get(commandsCtx).call(clearTextInCurrentBlockCommand.key);
		const view = ctx.get(editorViewCtx);
		const schema = ctx.get(schemaCtx);
		const at = view.state.selection.from;
		const input = document.createElement('input');
		input.type = 'file';
		input.accept = 'image/*';
		input.multiple = true;
		input.style.display = 'none';
		input.addEventListener('cancel', () => input.remove());
		input.addEventListener('change', async () => {
			const files = Array.from(input.files ?? []);
			input.remove();
			if (!files.length || view.isDestroyed) return;
			const id = Symbol('image pick');
			const pos = Math.min(at, view.state.doc.content.size);
			view.dispatch(view.state.tr.setMeta(pickKey, { add: { id, pos } }));
			const nodes = await uploadImages(files, schema);
			if (view.isDestroyed) return;
			const [placeholder] =
				pickKey
					.getState(view.state)
					?.find(undefined, undefined, (spec) => spec.id === id) ?? [];
			const tr = view.state.tr.setMeta(pickKey, { remove: id });
			if (placeholder && nodes.length) tr.insert(placeholder.from, nodes);
			view.dispatch(tr);
		});
		document.body.appendChild(input);
		input.click();
	};

	const crepe = new Crepe({
		root: options.root,
		defaultValue: options.markdown,
		features: {
			[Crepe.Feature.Latex]: false,
			[Crepe.Feature.ImageBlock]: false,
		},
		featureConfigs: {
			[Crepe.Feature.BlockEdit]: {
				handleAddIcon: icons.plus,
				handleDragIcon: icons.grip,
				blockHandle: { getOffset: () => 6 },
				buildMenu: (builder) => {
					if (!options.uploadFile) return;
					builder.getGroup('advanced').addItem('image', {
						label: 'Image',
						icon: icons.image,
						onRun: pickImages,
					});
				},
				slashMenu: {
					middleware: [
						shift({ padding: 8 }),
						size({
							padding: 8,
							apply: ({ availableHeight, elements }) => {
								elements.floating.style.setProperty(
									'--pe-menu-max',
									`${Math.max(160, availableHeight - 8)}px`,
								);
							},
						}),
					],
				},
				textGroup: {
					label: 'Text',
					text: item('Text', icons.text),
					h1: item('Heading 1', icons.h1),
					h2: item('Heading 2', icons.h2),
					h3: item('Heading 3', icons.h3),
					h4: null,
					h5: null,
					h6: null,
					quote: item('Quote', icons.quote),
					divider: item('Divider', icons.divider),
				},
				listGroup: {
					label: 'Lists',
					bulletList: item('Bulleted list', icons.bulletList),
					orderedList: item('Numbered list', icons.orderedList),
					taskList: item('Checklist', icons.taskList),
				},
				advancedGroup: {
					label: 'Insert',
					codeBlock: item('Code block', icons.code),
					table: item('Table', icons.table),
				},
			},
			[Crepe.Feature.Toolbar]: {
				boldIcon: icons.bold,
				italicIcon: icons.italic,
				strikethroughIcon: icons.strike,
				codeIcon: icons.code,
				linkIcon: icons.link,
				buildToolbar: (builder) => {
					builder.addGroup('tmgr', 'Page').addItem('make-task', {
						label: 'Make a task',
						icon: icons.makeTask,
						active: () => false,
						onRun: (ctx) => {
							const { state } = ctx.get(editorViewCtx);
							const { from, to } = state.selection;
							const text = state.doc.textBetween(from, to, '\n').trim();
							if (text) options.onMakeTask(text);
						},
					});
				},
			},
			[Crepe.Feature.ListItem]: {
				bulletIcon: icons.bullet,
				checkBoxCheckedIcon: icons.checked,
				checkBoxUncheckedIcon: icons.unchecked,
			},
			[Crepe.Feature.CodeMirror]: {
				languages,
				theme: syntaxHighlighting(classHighlighter),
				searchPlaceholder: 'Search language',
				noResultText: 'No language found',
			},
			[Crepe.Feature.Placeholder]: {
				text: options.placeholder,
				mode: 'block',
			},
		},
	});

	crepe.editor.use(pickPlaceholders);
	crepe.editor.config((ctx) => {
		ctx.update(remarkStringifyOptionsCtx, (prev) => ({
			...prev,
			bullet: '-' as const,
			rule: '-' as const,
		}));
		ctx.set(remarkGFMPlugin.options.key, { stringLength: () => 3 });
		ctx.update(uploadConfig.key, (prev) => ({
			...prev,
			uploader: uploadImages,
		}));
		ctx.set(linkAttr.key, (mark) => {
			const href = String(mark.attrs.href ?? '');
			const parsed = parseTmgrUrl(href);
			if (!parsed || parsed.form !== 'storage') {
				return parsed ? { 'data-tmgr': href, class: 'tmgr-chip' } : {};
			}
			const title = options.directory.titleFor(parsed.kind, parsed.id);
			return {
				'data-tmgr': href,
				class: `tmgr-chip tmgr-chip-${parsed.kind}`,
				...(title ? { 'data-current-title': title, 'aria-label': title } : {}),
			};
		});
	});

	return crepe;
};
