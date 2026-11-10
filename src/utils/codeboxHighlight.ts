import hljs from 'highlight.js/lib/common';
import apache from 'highlight.js/lib/languages/apache';
import coffeescript from 'highlight.js/lib/languages/coffeescript';
import http from 'highlight.js/lib/languages/http';
import nginx from 'highlight.js/lib/languages/nginx';
import properties from 'highlight.js/lib/languages/properties';

hljs.configure({ ignoreUnescapedHTML: true });
hljs.registerLanguage('apache', apache);
hljs.registerLanguage('coffeescript', coffeescript);
hljs.registerLanguage('http', http);
hljs.registerLanguage('nginx', nginx);
hljs.registerLanguage('properties', properties);

type HighlightGlobal = {
	highlightBlock: (el: HTMLElement) => void;
};

type CodeboxPrototype = {
	_injectHighlightJSScriptElement?: () => void;
};

export function bundleCodeboxHighlight(tool: { prototype: CodeboxPrototype }) {
	tool.prototype._injectHighlightJSScriptElement = () => {
		const bundled: HighlightGlobal = {
			highlightBlock: (el) => {
				delete el.dataset.highlighted;
				hljs.highlightElement(el);
			},
		};
		(window as unknown as { hljs: HighlightGlobal }).hljs = bundled;
	};
}
