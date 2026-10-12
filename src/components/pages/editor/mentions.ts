import { Plugin, PluginKey } from '@milkdown/kit/prose/state';
import type { EditorView } from '@milkdown/kit/prose/view';
import { $prose } from '@milkdown/kit/utils';
import { ref } from 'vue';
import type { MentionItem, TmgrDirectory } from '../useTmgrDirectory';

export interface MentionState {
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

const TRIGGER = /(?:^|\s)(@|\[\[)([^\s@[\]]{0,40})$/;

export const useMentions = (
	directory: () => TmgrDirectory,
	isReady: () => boolean,
	onPicked: (item: MentionItem) => void,
) => {
	const mention = ref<MentionState | null>(null);
	let searchSeq = 0;
	let searchTimer: ReturnType<typeof setTimeout> | undefined;
	let viewRef: EditorView | null = null;

	const closeMention = () => {
		clearTimeout(searchTimer);
		searchSeq++;
		mention.value = null;
	};

	const runSearch = (state: MentionState) => {
		clearTimeout(searchTimer);
		const seq = ++searchSeq;
		searchTimer = setTimeout(async () => {
			const items = await directory().searchMentions(
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
		directory().titles[`${item.kind}/${item.id}`] = item.title;
		const label = schema.text(item.title, [link]);
		const tr = view.state.tr.replaceWith(state.from, state.to, label);
		tr.insert(state.from + item.title.length, schema.text(' '));
		view.dispatch(tr);
		onPicked(item);
		closeMention();
		view.focus();
	};

	const plugin = $prose(
		() =>
			new Plugin({
				key: new PluginKey('tmgr-mention'),
				view: () => ({
					update(view) {
						viewRef = view;
						const { selection } = view.state;
						if (!selection.empty || !isReady()) {
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
								(state.index + step + state.items.length) % state.items.length;
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

	const dispose = () => {
		clearTimeout(searchTimer);
	};

	return { mention, pick, plugin, dispose };
};
