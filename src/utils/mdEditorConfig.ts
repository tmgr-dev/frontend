import { XSSPlugin, config } from 'md-editor-v3';

// The embedUrl hosts of the services enabled for the Editor.js embed tool in
// BlockEditor.vue, so both editors embed the same things.
const IFRAME_HOSTS = new Set(['www.youtube.com', 'codepen.io', 'www.figma.com']);

// The app origin is never in IFRAME_HOSTS, so `allow-same-origin` only gives the
// embed its own origin back. An embed can still navigate itself to the app origin
// and keeps these flags, which is safe only while the app origin serves no
// user-controlled HTML.
const IFRAME_SANDBOX = 'allow-scripts allow-same-origin allow-popups';

const decodeEntities = (value: string) => {
	const textarea = document.createElement('textarea');
	textarea.innerHTML = value;
	return textarea.value;
};

const escapeAttr = (value: string) =>
	value
		.replace(/&/g, '&amp;')
		.replace(/"/g, '&quot;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;');

// The returned string replaces the attribute: the src goes out as the parsed URL
// (what was checked is what loads) with the sandbox, or not at all. An author
// `sandbox` or `srcdoc` is not whitelisted and is dropped by the filter.
const iframeSrc = (value: string) => {
	let url: URL;
	try {
		url = new URL(decodeEntities(value).trim());
	} catch {
		return '';
	}
	if (
		url.protocol !== 'https:' ||
		!IFRAME_HOSTS.has(url.host) ||
		url.username ||
		url.password
	)
		return '';
	return `src="${escapeAttr(url.href)}" sandbox="${IFRAME_SANDBOX}"`;
};

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
		{
			type: 'xss',
			plugin: XSSPlugin,
			options: {
				xss: {
					// `allow` would let an embed's scripts ask for camera, microphone,
					// location or the clipboard in the app's name.
					onTagAttr: (tag: string, name: string, value: string) => {
						if (tag !== 'iframe') return undefined;
						if (name === 'src') return iframeSrc(value);
						if (name === 'allow') return '';
						return undefined;
					},
				},
			},
		},
	],
});
