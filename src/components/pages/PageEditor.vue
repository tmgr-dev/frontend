<template>
	<div class="page-editor relative" ref="rootRef" @click.capture="onClick">
		<div ref="mountRef"></div>
		<Teleport to="body">
			<ul
				v-if="mention && (mention.items.length || mention.loading)"
				class="mention-menu fixed z-50 max-h-72 w-72 overflow-y-auto rounded-lg border border-gray-200 bg-white py-1 text-sm shadow-lg dark:border-gray-700 dark:bg-gray-800"
				:style="{ left: `${mention.x}px`, top: `${mention.y}px` }"
				role="listbox"
				data-testid="mention-menu"
			>
				<li
					v-for="(item, index) in mention.items"
					:key="`${item.kind}/${item.id}`"
					role="option"
					:aria-selected="index === mention.index"
					class="flex cursor-pointer items-center gap-2 px-3 py-1.5"
					:class="
						index === mention.index
							? 'bg-blue-50 text-blue-900 dark:bg-blue-900/30 dark:text-blue-100'
							: 'text-gray-800 hover:bg-gray-50 dark:text-gray-200 dark:hover:bg-gray-700'
					"
					@mousedown.prevent="pick(item)"
				>
					<span
						class="w-16 shrink-0 text-2xs uppercase tracking-wide text-gray-400 dark:text-gray-500"
						>{{ item.kind }}</span
					>
					<span class="truncate">{{ item.title }}</span>
				</li>
				<li
					v-if="mention.loading && !mention.items.length"
					class="px-3 py-1.5 text-gray-400 dark:text-gray-500"
				>
					...
				</li>
			</ul>
		</Teleport>
	</div>
</template>

<script lang="ts">
	import { fileDisplayUrl, releaseFileDisplayUrl } from '@/actions/tmgr/files';
	import store from '@/store';
	import { extractTmgrRefs, parseTmgrUrl } from '@/utils/pages/tmgrLinks';
	import { Crepe } from '@milkdown/crepe';
	import '@milkdown/crepe/theme/common/style.css';
	import darkTheme from '@milkdown/crepe/theme/frame-dark.css?inline';
	import lightTheme from '@milkdown/crepe/theme/frame.css?inline';
	import { imageSchema, linkAttr } from '@milkdown/kit/preset/commonmark';
	import { Plugin, PluginKey } from '@milkdown/kit/prose/state';
	import { $prose, $view } from '@milkdown/kit/utils';
	import {
		defineComponent,
		onBeforeUnmount,
		onMounted,
		type PropType,
		ref,
		watch,
	} from 'vue';
	import type { MentionItem, TmgrDirectory } from './useTmgrDirectory';

	interface MentionState {
		items: MentionItem[];
		index: number;
		x: number;
		y: number;
		from: number;
		to: number;
		query: string;
		pagesOnly: boolean;
		loading: boolean;
	}

	const themeStyle = document.createElement('style');
	themeStyle.dataset.pagesEditorTheme = '';
	let themeUsers = 0;
	const applyTheme = () => {
		themeStyle.textContent = store.getters.isDarkTheme ? darkTheme : lightTheme;
	};
	const acquireTheme = () => {
		if (themeUsers++ === 0) {
			applyTheme();
			document.head.appendChild(themeStyle);
		}
	};
	const releaseTheme = () => {
		if (--themeUsers === 0) themeStyle.remove();
	};

	const TRIGGER = /(?:^|\s)(@|\[\[)([^\s@[\]]{0,40})$/;

	export default defineComponent({
		name: 'PageEditor',
		props: {
			modelValue: { type: String, default: '' },
			directory: { type: Object as PropType<TmgrDirectory>, required: true },
			placeholder: { type: String, default: '' },
		},
		emits: ['change', 'navigate'],
		setup(props, { emit }) {
			const rootRef = ref<HTMLElement | null>(null);
			const mountRef = ref<HTMLElement | null>(null);
			const mention = ref<MentionState | null>(null);
			let crepe: Crepe | null = null;
			let baseline = '';
			let ready = false;
			let searchSeq = 0;
			let searchTimer: ReturnType<typeof setTimeout> | undefined;
			let viewRef: any = null;

			const closeMention = () => {
				clearTimeout(searchTimer);
				searchSeq++;
				mention.value = null;
			};

			const runSearch = (state: MentionState) => {
				clearTimeout(searchTimer);
				const seq = ++searchSeq;
				searchTimer = setTimeout(async () => {
					const items = await props.directory.searchMentions(
						state.query,
						state.pagesOnly,
					);
					if (seq !== searchSeq || !mention.value) return;
					mention.value = {
						...mention.value,
						items,
						index: 0,
						loading: false,
					};
				}, 120);
			};

			const pick = (item: MentionItem) => {
				const state = mention.value;
				const view = viewRef;
				if (!state || !view) return;
				const { schema } = view.state;
				const link = schema.marks.link.create({
					href: `tmgr://${item.kind}/${item.id}`,
				});
				const label = schema.text(item.title, [link]);
				const tr = view.state.tr.replaceWith(state.from, state.to, label);
				tr.insert(state.from + item.title.length, schema.text(' '));
				view.dispatch(tr);
				props.directory.titles[`${item.kind}/${item.id}`] = item.title;
				closeMention();
				view.focus();
			};

			const mentionPlugin = $prose(
				() =>
					new Plugin({
						key: new PluginKey('tmgr-mention'),
						view: () => ({
							update(view) {
								viewRef = view;
								const { selection } = view.state;
								if (!selection.empty || !ready) {
									if (mention.value) closeMention();
									return;
								}
								const $from = selection.$from;
								if ($from.parent.type.spec.code) {
									if (mention.value) closeMention();
									return;
								}
								const before = $from.parent.textBetween(
									Math.max(0, $from.parentOffset - 60),
									$from.parentOffset,
									undefined,
									'￼',
								);
								const match = TRIGGER.exec(before);
								if (!match) {
									if (mention.value) closeMention();
									return;
								}
								const to = $from.pos;
								const from = to - match[1].length - match[2].length;
								const coords = view.coordsAtPos(to);
								const previous = mention.value;
								const sameQuery =
									previous &&
									previous.query === match[2] &&
									previous.pagesOnly === (match[1] === '[[');
								const next: MentionState = {
									items: previous?.items ?? [],
									index: sameQuery ? previous.index : 0,
									x: coords.left,
									y: coords.bottom + 4,
									from,
									to,
									query: match[2],
									pagesOnly: match[1] === '[[',
									loading: sameQuery ? previous.loading : true,
								};
								mention.value = next;
								if (!sameQuery) runSearch(next);
							},
							destroy() {
								viewRef = null;
							},
						}),
						props: {
							handleKeyDown(_view, event) {
								const state = mention.value;
								if (!state) return false;
								if (event.key === 'Escape') {
									closeMention();
									return true;
								}
								if (!state.items.length) return false;
								if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
									const step = event.key === 'ArrowDown' ? 1 : -1;
									state.index =
										(state.index + step + state.items.length) %
										state.items.length;
									return true;
								}
								if (event.key === 'Enter' || event.key === 'Tab') {
									pick(state.items[state.index]);
									return true;
								}
								return false;
							},
						},
					}),
			);

			const imageView = $view(imageSchema.node, () => (node) => {
				const img = document.createElement('img');
				img.draggable = true;
				let shown: string | null = null;
				let current = '';
				const apply = (src: string, alt: string) => {
					img.alt = alt;
					if (src === current) return;
					current = src;
					const parsed = parseTmgrUrl(src);
					if (parsed?.form === 'storage' && parsed.kind === 'file') {
						fileDisplayUrl(Number(parsed.id))
							.then((url) => {
								if (current !== src) {
									releaseFileDisplayUrl(url);
									return;
								}
								releaseFileDisplayUrl(shown);
								shown = url;
								img.src = url;
							})
							.catch(() => undefined);
					} else {
						img.src = src;
					}
				};
				apply(node.attrs.src, node.attrs.alt);
				return {
					dom: img,
					update(updated) {
						if (updated.type !== node.type) return false;
						apply(updated.attrs.src, updated.attrs.alt);
						return true;
					},
					destroy() {
						current = '';
						releaseFileDisplayUrl(shown);
					},
				};
			});

			const onClick = (event: MouseEvent) => {
				const target = event.target as HTMLElement | null;
				const anchor = target?.closest?.('a') as HTMLAnchorElement | null;
				if (!anchor) return;
				const href = anchor.getAttribute('data-tmgr');
				if (href === null) {
					if (anchor.getAttribute('href') === '') event.preventDefault();
					return;
				}
				event.preventDefault();
				event.stopPropagation();
				const parsed = parseTmgrUrl(href);
				if (parsed) emit('navigate', parsed);
			};

			const rebuildTheme = () => applyTheme();

			onMounted(async () => {
				acquireTheme();
				await props.directory.ensure(extractTmgrRefs(props.modelValue));
				if (!mountRef.value) return;
				crepe = new Crepe({
					root: mountRef.value,
					defaultValue: props.modelValue,
					features: {
						[Crepe.Feature.CodeMirror]: false,
						[Crepe.Feature.Latex]: false,
						[Crepe.Feature.ImageBlock]: false,
					},
					featureConfigs: {
						[Crepe.Feature.BlockEdit]: {
							blockHandle: {
								getPlacement: () => 'right-start',
								getOffset: () => ({ mainAxis: 8 }),
							},
						},
						[Crepe.Feature.Placeholder]: {
							text: props.placeholder,
							mode: 'block',
						},
					},
				});
				crepe.editor
					.config((ctx) => {
						ctx.set(linkAttr.key, (mark) => {
							const href = String(mark.attrs.href ?? '');
							const parsed = parseTmgrUrl(href);
							if (!parsed || parsed.form !== 'storage') {
								return parsed ? { 'data-tmgr': href, class: 'tmgr-chip' } : {};
							}
							const title = props.directory.titleFor(parsed.kind, parsed.id);
							return {
								'data-tmgr': href,
								class: `tmgr-chip tmgr-chip-${parsed.kind}`,
								...(title
									? { 'data-current-title': title, 'aria-label': title }
									: {}),
							};
						});
					})
					.use(mentionPlugin)
					.use(imageView);
				crepe.on((listener) => {
					listener.markdownUpdated((_ctx, markdown) => {
						if (!ready) return;
						emit('change', markdown, markdown !== baseline);
					});
				});
				await crepe.create();
				baseline = crepe.getMarkdown();
				ready = true;
			});

			watch(() => store.getters.isDarkTheme, rebuildTheme);

			onBeforeUnmount(() => {
				clearTimeout(searchTimer);
				ready = false;
				crepe?.destroy();
				crepe = null;
				releaseTheme();
			});

			return { rootRef, mountRef, mention, pick, onClick };
		},
	});
</script>

<style>
	.page-editor .milkdown {
		--crepe-color-background: transparent;
		--crepe-color-selected: var(--selection-bg);
		background: transparent;
	}

	.page-editor .milkdown-slash-menu .menu-groups {
		max-height: 240px;
		overflow-y: auto;
	}

	.page-editor .milkdown .ProseMirror {
		padding: 8px 80px 16px 16px;
		min-height: 48px;
	}

	.page-editor a.tmgr-chip {
		display: inline;
		border-radius: 9999px;
		background: rgb(59 130 246 / 0.12);
		padding: 0 0.5em;
		color: rgb(29 78 216);
		text-decoration: none;
		cursor: pointer;
	}

	.dark .page-editor a.tmgr-chip {
		background: rgb(96 165 250 / 0.18);
		color: rgb(147 197 253);
	}

	.page-editor a.tmgr-chip {
		position: relative;
	}

	.page-editor a.tmgr-chip[data-current-title]:hover::after {
		content: attr(data-current-title);
		position: absolute;
		left: 0;
		top: 100%;
		z-index: 20;
		margin-top: 2px;
		max-width: 20rem;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		border-radius: 0.375rem;
		background: rgb(31 41 55);
		padding: 0.15rem 0.5rem;
		font-size: 0.75rem;
		color: white;
		pointer-events: none;
	}

	.page-editor img {
		max-width: 100%;
		border-radius: 0.375rem;
	}
</style>
