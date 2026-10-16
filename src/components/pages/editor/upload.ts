import { fileDisplayUrl, releaseFileDisplayUrl } from '@/actions/tmgr/files';
import { imageFilesOf } from '@/utils/pages/files';
import {
	formatImageTitle,
	parseImageTitle,
	pngPixelRatio,
} from '@/utils/pages/imageTitle';
import type { Node } from '@milkdown/kit/prose/model';
import {
	NodeSelection,
	Plugin,
	PluginKey,
	type Transaction,
} from '@milkdown/kit/prose/state';
import { Mapping, ReplaceStep } from '@milkdown/kit/prose/transform';
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
	url: string;
}

interface Range {
	from: number;
	to: number;
}

interface GhostState {
	set: DecorationSet;
	targets: Map<number, number>;
	ranges: Map<number, Range>;
}

interface Size {
	width: number | null;
	height: number | null;
}

interface GhostMeta {
	add?: Ghost[];
	track?: { id: number; pos: number };
	range?: Range & { id: number };
	remove?: number[];
}

const ghostKey = new PluginKey<GhostState>('tmgr-image-upload');
const PRELOAD_TIMEOUT = 8000;
let lastId = 0;

// Resizing, aligning or captioning re-creates the image in place, which plain
// position mapping reports as a deletion; only a real deletion drops the target.
const mapTarget = (tr: Transaction, pos: number): number | null => {
	let at = pos;
	for (const step of tr.steps) {
		if (
			step instanceof ReplaceStep &&
			step.from === at &&
			step.to === at + 1 &&
			step.slice.openStart === 0 &&
			step.slice.openEnd === 0 &&
			step.slice.content.childCount === 1 &&
			step.slice.content.firstChild?.type.name === 'image'
		) {
			continue;
		}
		const result = step.getMap().mapResult(at, 1);
		if (result.deletedAfter) return null;
		at = result.pos;
	}
	return at;
};

const ghostDecoration = (ghost: Ghost): Decoration => {
	const wrap = document.createElement('span');
	wrap.className = 'pe-upload-ghost';
	wrap.contentEditable = 'false';
	const img = document.createElement('img');
	img.alt = '';
	img.src = ghost.url;
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
		?.set.find(undefined, undefined, (spec) => spec.id === id)[0];

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

// Text around an <img> (a web page, a doc) goes through the normal paste;
// only a bare copied image is uploaded from the clipboard file.
const isImageOnlyHtml = (html: string): boolean => {
	const body = new DOMParser().parseFromString(html, 'text/html').body;
	return !!body.querySelector('img') && !(body.textContent ?? '').trim();
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
			new Plugin<GhostState>({
				key: ghostKey,
				state: {
					init: () => ({
						set: DecorationSet.empty,
						targets: new Map(),
						ranges: new Map(),
					}),
					apply(tr, prev) {
						let set = prev.set.map(tr.mapping, tr.doc);
						let { targets, ranges } = prev;
						if (tr.docChanged && targets.size) {
							targets = new Map();
							for (const [id, pos] of prev.targets) {
								const at = mapTarget(tr, pos);
								if (at !== null) targets.set(id, at);
							}
						}
						if (tr.docChanged && ranges.size) {
							ranges = new Map();
							for (const [id, range] of prev.ranges) {
								const from = tr.mapping.map(range.from, 1);
								const to = tr.mapping.map(range.to, -1);
								if (from < to) ranges.set(id, { from, to });
							}
						}
						const meta = tr.getMeta(ghostKey) as GhostMeta | undefined;
						if (meta?.add) set = set.add(tr.doc, meta.add.map(ghostDecoration));
						if (meta?.track) {
							const { id, pos } = meta.track;
							targets = new Map(
								[...targets].filter(([, at]) => at !== pos),
							).set(id, pos);
						}
						if (meta?.range) {
							const { id, from, to } = meta.range;
							ranges = new Map(ranges).set(id, { from, to });
						}
						if (meta?.remove?.length) {
							const gone = new Set(meta.remove);
							set = set.remove(
								set.find(undefined, undefined, (spec) => gone.has(spec.id)),
							);
							targets = new Map([...targets].filter(([id]) => !gone.has(id)));
							ranges = new Map([...ranges].filter(([id]) => !gone.has(id)));
						}
						return { set, targets, ranges };
					},
				},
				props: {
					decorations: (state) => {
						const ghosts = ghostKey.getState(state);
						if (!ghosts?.targets.size) return ghosts?.set;
						const dimmed = [...ghosts.targets.values()]
							.filter((at) => state.doc.nodeAt(at)?.type.name === 'image')
							.map((at) =>
								Decoration.node(at, at + 1, { class: 'pe-image-replacing' }),
							);
						return ghosts.set.add(state.doc, dimmed);
					},
				},
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

	// `replacing` is the pasted-over selection: it stays until an image lands.
	const insertImages = async (
		view: EditorView,
		pos: number,
		list: File[],
		replacing?: Range,
	) => {
		const files = imageFilesOf(list);
		if (!options.uploadFile || !files.length || view.isDestroyed) return;
		const jobs = files.map((file) => ({
			id: ++lastId,
			file,
			url: URL.createObjectURL(file),
		}));
		const rangeId = ++lastId;
		view.dispatch(
			view.state.tr.setMeta(ghostKey, {
				add: jobs.map(({ id, url }) => ({ id, pos, url })),
				range: replacing && { id: rangeId, ...replacing },
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
			const remove = [job.id];
			const tr = view.state.tr;
			if (ghost && node) {
				const inline = tr.doc.resolve(ghost.from).parent.inlineContent;
				tr.insert(
					ghost.from,
					inline ? node : view.state.schema.nodes.paragraph.create(null, node),
				);
				const range = ghostKey.getState(view.state)?.ranges.get(rangeId);
				if (range) {
					tr.delete(
						tr.mapping.map(range.from, 1),
						tr.mapping.map(range.to, -1),
					);
					remove.push(rangeId);
				}
			}
			if (index === jobs.length - 1) remove.push(rangeId);
			view.dispatch(tr.setMeta(ghostKey, { remove }));
		}
	};

	const replaceImage = async (view: EditorView, pos: number, file: File) => {
		const current = view.state.doc.nodeAt(pos);
		if (!options.uploadFile || current?.type.name !== 'image') return;
		const id = ++lastId;
		const url = URL.createObjectURL(file);
		view.dispatch(view.state.tr.setMeta(ghostKey, { track: { id, pos } }));
		let result: Awaited<ReturnType<typeof upload>> | null = null;
		try {
			result = await upload(file, url);
		} catch {
			options.onUploadError(file);
		}
		URL.revokeObjectURL(url);
		if (view.isDestroyed) return;
		const at = ghostKey.getState(view.state)?.targets.get(id);
		const tr = view.state.tr.setMeta(ghostKey, { remove: [id] });
		const target = at === undefined ? null : tr.doc.nodeAt(at);
		if (result && at !== undefined && target?.type.name === 'image') {
			const parts = parseImageTitle(target.attrs.title);
			const { width, height } = result.size;
			const keepWidth = parts.width ?? width;
			const fitted =
				keepWidth && width && height
					? {
							width: keepWidth,
							height: Math.round((keepWidth * height) / width),
					  }
					: { width, height };
			tr.setNodeMarkup(at, undefined, {
				...target.attrs,
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
		if (html && !isImageOnlyHtml(html)) return false;
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
		const { from, to, empty } = selection;
		void insertImages(
			view,
			empty ? from : to,
			files,
			empty ? undefined : { from, to },
		);
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
