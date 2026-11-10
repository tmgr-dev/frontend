import hljs from 'highlight.js/lib/common';

type HighlightGlobal = {
	highlightBlock: (el: HTMLElement) => void;
};

type CodeboxPrototype = {
	_injectHighlightJSScriptElement?: () => void;
};

export function bundleCodeboxHighlight(tool: { prototype: CodeboxPrototype }) {
	tool.prototype._injectHighlightJSScriptElement = () => {
		const bundled: HighlightGlobal = {
			highlightBlock: (el) => hljs.highlightElement(el),
		};
		(window as unknown as { hljs: HighlightGlobal }).hljs = bundled;
	};
}
