import { fileDisplayUrl, releaseFileDisplayUrl } from '@/actions/tmgr/files';
import { imageFilesOf } from '@/utils/pages/files';
import {
	formatImageTitle,
	parseImageTitle,
	pngPixelRatio,
} from '@/utils/pages/imageTitle';
import type { Node } from '@milkdown/kit/prose/model';
import { NodeSelection, Plugin, PluginKey } from '@milkdown/kit/prose/state';
import { Mapping } from '@milkdown/kit/prose/transform';
import {
	Decoration,
	DecorationSet,
	type EditorView,
} from '@milkdown/kit/prose/view';
import { $prose } from '@milkdown/kit/utils';

export interface ImageUploadOptions {
	uploadFile?: (file: File) => Promise<number>;
	onUploadError: (file: File) => void;
}

interface Ghost {
	id: number;
	pos: number;
	url?: string;
	replaceSize?: number;
}

interface Size {
	width: number | null;
	height: number | null;
}

type GhostMeta = { add: Ghost[] } | { remove: number };

const ghostKey = new PluginKey<DecorationSet>('tmgr-image-upload');
const PRELOAD_TIMEOUT = 8000;
let lastId = 0;

const ghostDecoration = (ghost: Ghost): Decoration => {
	if (ghost.replaceSize) {
		return Decoration.node(
			ghost.pos,
			ghost.pos + ghost.replaceSize,
			{ class: 'pe-image-replacing' },
			{ id: ghost.id },
		);
	}
	const wrap = document.createElement('span');
	wrap.className = 'pe-upload-ghost';
	wrap.contentEditable = 'false';
	const img = document.createElement('img');
	img.alt = '';
	if (ghost.url) img.src = ghost.url;
	wrap.append(img);
	return Decoration.widget(ghost.pos, wrap, {
		id: ghost.id,
		key: `pe-upload-ghost-${ghost.id}`,
		ignoreSelection: true,
	});
};

const findGhost = (view: EditorView, id: number): Decoration | undefined =>
	ghostKey
		.getState(view.state)
		?.find(undefined, undefined, (spec) => spec.id === id)[0];

const measure = async (file: File, url: string): Promise<Size> => {
	const img = new Image();
	img.src = url;
	try {
		await img.decode();
	} catch {
		return { width: null, height: null };
	}
	if (!img.naturalWidth || !img.naturalHeight) {
		return { width: null, height: null };
	}
	let ratio = 1;
	if (file.type === 'image/png') {
		try {
			ratio = pngPixelRatio(
				new Uint8Array(await file.slice(0, 65536).arrayBuffer()),
			);
		} catch {
			ratio = 1;
		}
	}
	return {
		width: Math.round(img.naturalWidth / ratio),
		height: Math.round(img.naturalHeight / ratio),
	};
};

const preload = async (fileId: number): Promise<void> => {
	let url: string | null = null;
	try {
		url = await fileDisplayUrl(fileId);
		const img = new Image();
		img.src = url;
		await Promise.race([
			img.decode(),
			new Promise((resolve) => setTimeout(resolve, PRELOAD_TIMEOUT)),
		]);
	} catch {
		// the node view shows its own error state if the image can't load
	} finally {
		releaseFileDisplayUrl(url);
	}
};

export const pickImageFiles = (multiple: boolean): Promise<File[]> =>
	new Promise((resolve) => {
		const input = document.createElement('input');
		input.type = 'file';
		input.accept = 'image/*';
		input.multiple = multiple;
		input.style.display = 'none';
		const done = (files: File[]) => {
			input.remove();
			resolve(files);
		};
		input.addEventListener('cancel', () => done([]));
		input.addEventListener('change', () => done(Array.from(input.files ?? [])));
		document.body.appendChild(input);
		input.click();
	});

export const createImageUpload = (options: ImageUploadOptions) => {
	const plugin = $prose(
		() =>
			new Plugin<DecorationSet>({
				key: ghostKey,
				state: {
					init: () => DecorationSet.empty,
					apply(tr, set) {
						let next = set.map(tr.mapping, tr.doc);
						const meta = tr.getMeta(ghostKey) as GhostMeta | undefined;
						if (meta && 'add' in meta) {
							next = next.add(tr.doc, meta.add.map(ghostDecoration));
						} else if (meta && 'remove' in meta) {
							next = next.remove(
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
				props: { decorations: (state) => ghostKey.getState(state) },
				appendTransaction: (trs, oldState, newState) => {
					if (
						!trs.some((tr) => tr.docChanged && tr.getMeta('uiEvent') === 'drop')
					)
						return null;
					const mapping = new Mapping();
					for (const tr of trs) mapping.appendMapping(tr.mapping);
					const emptied = new Set<number>();
					oldState.doc.descendants((node, pos) => {
						if (node.type.name !== 'paragraph') return true;
						if (
							node.childCount === 1 &&
							node.firstChild?.type.name === 'image'
						) {
							const at = mapping.map(pos, 1);
							const now = newState.doc.nodeAt(at);
							if (now?.type.name === 'paragraph' && now.childCount === 0)
								emptied.add(at);
						}
						return false;
					});
					if (!emptied.size || newState.doc.childCount <= emptied.size)
						return null;
					const tr = newState.tr;
					for (const at of [...emptied].sort((a, b) => b - a)) {
						tr.delete(at, at + 2);
					}
					return tr;
				},
			}),
	);

	const upload = async (file: File, url: string) => {
		const [fileId, size] = await Promise.all([
			options.uploadFile!(file),
			measure(file, url),
		]);
		await preload(fileId);
		return { fileId, size };
	};

	const insertImages = async (view: EditorView, pos: number, list: File[]) => {
		const files = imageFilesOf(list);
		if (!options.uploadFile || !files.length || view.isDestroyed) return;
		const jobs = files.map((file) => ({
			id: ++lastId,
			file,
			url: URL.createObjectURL(file),
		}));
		view.dispatch(
			view.state.tr.setMeta(ghostKey, {
				add: jobs.map(({ id, url }) => ({ id, pos, url })),
			}),
		);
		const results = jobs.map((job) =>
			upload(job.file, job.url).catch(() => null),
		);
		for (const [index, job] of jobs.entries()) {
			let node: Node | null = null;
			const result = await results[index];
			if (result) {
				node = view.state.schema.nodes.image.create({
					src: `tmgr://file/${result.fileId}`,
					alt: '',
					title: formatImageTitle({ ...parseImageTitle(''), ...result.size }),
				});
			} else {
				options.onUploadError(job.file);
			}
			URL.revokeObjectURL(job.url);
			if (view.isDestroyed) continue;
			const ghost = findGhost(view, job.id);
			const tr = view.state.tr.setMeta(ghostKey, { remove: job.id });
			if (ghost && node) {
				const inline = tr.doc.resolve(ghost.from).parent.inlineContent;
				tr.insert(
					ghost.from,
					inline ? node : view.state.schema.nodes.paragraph.create(null, node),
				);
			}
			view.dispatch(tr);
		}
	};

	const replaceImage = async (view: EditorView, pos: number, file: File) => {
		const current = view.state.doc.nodeAt(pos);
		if (!options.uploadFile || current?.type.name !== 'image') return;
		const id = ++lastId;
		const url = URL.createObjectURL(file);
		view.dispatch(
			view.state.tr.setMeta(ghostKey, {
				add: [{ id, pos, replaceSize: current.nodeSize }],
			}),
		);
		let result: Awaited<ReturnType<typeof upload>> | null = null;
		try {
			result = await upload(file, url);
		} catch {
			options.onUploadError(file);
		}
		URL.revokeObjectURL(url);
		if (view.isDestroyed) return;
		const tr = view.state.tr.setMeta(ghostKey, { remove: id });
		// Resizing or aligning re-creates the node and drops its decoration, so
		// the image is found again by its source, nearest to where it was.
		let found: { node: Node; pos: number } | null = null;
		tr.doc.descendants((node, at) => {
			if (
				node.type.name === 'image' &&
				node.attrs.src === current.attrs.src &&
				(!found || Math.abs(at - pos) < Math.abs(found.pos - pos))
			) {
				found = { node, pos: at };
			}
		});
		const target = found as { node: Node; pos: number } | null;
		if (target && result) {
			const parts = parseImageTitle(target.node.attrs.title);
			const { width, height } = result.size;
			const keepWidth = parts.width ?? width;
			const fitted =
				keepWidth && width && height
					? {
							width: keepWidth,
							height: Math.round((keepWidth * height) / width),
					  }
					: { width, height };
			tr.setNodeMarkup(target.pos, undefined, {
				...target.node.attrs,
				src: `tmgr://file/${result.fileId}`,
				title: formatImageTitle({ ...parts, ...fitted }),
			});
		}
		view.dispatch(tr);
	};

	const handlePaste = (view: EditorView, event: ClipboardEvent): boolean => {
		const data = event.clipboardData;
		const files = imageFilesOf(data?.files);
		if (!options.uploadFile || !files.length) return false;
		const html = data?.getData('text/html') ?? '';
		if (html && !/<img\b/i.test(html)) return false;
		event.preventDefault();
		const { selection } = view.state;
		if (
			selection instanceof NodeSelection &&
			selection.node.type.name === 'image' &&
			files.length === 1
		) {
			void replaceImage(view, selection.from, files[0]);
			return true;
		}
		if (!selection.empty) view.dispatch(view.state.tr.deleteSelection());
		void insertImages(view, view.state.selection.from, files);
		return true;
	};

	const handleDrop = (
		view: EditorView,
		event: DragEvent,
		_slice: unknown,
		moved: boolean,
	): boolean => {
		const files = imageFilesOf(event.dataTransfer?.files);
		if (moved || !options.uploadFile || !files.length) return false;
		const at = view.posAtCoords({ left: event.clientX, top: event.clientY });
		if (!at) return false;
		event.preventDefault();
		void insertImages(view, at.pos, files);
		return true;
	};

	return { plugin, insertImages, replaceImage, handlePaste, handleDrop };
};
