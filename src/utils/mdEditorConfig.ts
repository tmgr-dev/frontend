import { XSSPlugin, config } from 'md-editor-v3';

// Global md-editor-v3 config, shared by Editor.vue and DescriptionMarkdown.vue
// (the preview can render before the editor is ever loaded).
config({
	// Page fullscreen only: a stub instance keeps the library from loading screenfull from a CDN.
	editorExtensions: {
		screenfull: { instance: { isEnabled: false } },
		// The default parser evals the block with `new Function`; descriptions are untrusted.
		echarts: { parseOption: (code) => JSON.parse(code) },
	},
	// Since 5.0 XSS filtering is opt-in while markdown-it runs with `html: true`;
	// descriptions come from other workspace members, so sanitise every render.
	markdownItPlugins: (plugins) => [
		...plugins,
		{ type: 'xss', plugin: XSSPlugin, options: {} },
	],
});
