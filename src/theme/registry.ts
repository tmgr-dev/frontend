export interface ThemeTokens {
	bg: string;
	surface: string;
	raised: string;
	border: string;
	text: string;
	muted: string;
	brand: string;
	timer: string;
	onAccent: string;
}
export interface ThemeDef {
	id: string;
	name: string;
	isDark: boolean;
	tokens: ThemeTokens;
}

export const DEFAULT_THEME = 'default';

export const THEME_REGISTRY: ThemeDef[] = [
	{ id: 'dracula', name: 'Dracula', isDark: true, tokens: { bg:'#282a36', surface:'#21222c', raised:'#343746', border:'#44475a', text:'#f8f8f2', muted:'#6272a4', brand:'#bd93f9', timer:'#50fa7b', onAccent:'#282a36' } },
	{ id: 'nord', name: 'Nord', isDark: true, tokens: { bg:'#2e3440', surface:'#272c36', raised:'#3b4252', border:'#434c5e', text:'#eceff4', muted:'#7b88a1', brand:'#88c0d0', timer:'#a3be8c', onAccent:'#2e3440' } },
	{ id: 'one-dark', name: 'One Dark', isDark: true, tokens: { bg:'#282c34', surface:'#21252b', raised:'#2c313a', border:'#3b4048', text:'#dcdfe4', muted:'#5c6370', brand:'#61afef', timer:'#98c379', onAccent:'#282c34' } },
	{ id: 'monokai', name: 'Monokai', isDark: true, tokens: { bg:'#272822', surface:'#1e1f1c', raised:'#34352f', border:'#414339', text:'#f8f8f2', muted:'#75715e', brand:'#f92672', timer:'#a6e22e', onAccent:'#272822' } },
	{ id: 'solarized-dark', name: 'Solarized Dark', isDark: true, tokens: { bg:'#002b36', surface:'#00252e', raised:'#073642', border:'#0f4a56', text:'#eee8d5', muted:'#5a7681', brand:'#268bd2', timer:'#859900', onAccent:'#002b36' } },
	{ id: 'gruvbox-dark', name: 'Gruvbox Dark', isDark: true, tokens: { bg:'#282828', surface:'#1d2021', raised:'#3c3836', border:'#504945', text:'#ebdbb2', muted:'#928374', brand:'#fabd2f', timer:'#b8bb26', onAccent:'#282828' } },
	{ id: 'tokyo-night', name: 'Tokyo Night', isDark: true, tokens: { bg:'#1a1b26', surface:'#16161e', raised:'#24283b', border:'#2f334d', text:'#c0caf5', muted:'#565f89', brand:'#7aa2f7', timer:'#9ece6a', onAccent:'#1a1b26' } },
	{ id: 'github-dark', name: 'GitHub Dark', isDark: true, tokens: { bg:'#0d1117', surface:'#010409', raised:'#161b22', border:'#30363d', text:'#e6edf3', muted:'#7d8590', brand:'#58a6ff', timer:'#3fb950', onAccent:'#0d1117' } },
	{ id: 'night-owl', name: 'Night Owl', isDark: true, tokens: { bg:'#011627', surface:'#010f1c', raised:'#0b2942', border:'#1d3b53', text:'#d6deeb', muted:'#637777', brand:'#82aaff', timer:'#22da6e', onAccent:'#011627' } },
	{ id: 'cobalt2', name: 'Cobalt2', isDark: true, tokens: { bg:'#193549', surface:'#15232d', raised:'#1e415e', border:'#0d5289', text:'#ffffff', muted:'#7b93a8', brand:'#ffc600', timer:'#3ad900', onAccent:'#193549' } },
	{ id: 'palenight', name: 'Palenight', isDark: true, tokens: { bg:'#292d3e', surface:'#232635', raised:'#333747', border:'#444a63', text:'#d2d6ec', muted:'#676e95', brand:'#c792ea', timer:'#c3e88d', onAccent:'#292d3e' } },
	{ id: 'ayu-dark', name: 'Ayu Dark', isDark: true, tokens: { bg:'#0b0e14', surface:'#080a0f', raised:'#131721', border:'#1f2430', text:'#bfbdb6', muted:'#565b66', brand:'#ffb454', timer:'#7fd962', onAccent:'#0b0e14' } },
	{ id: 'catppuccin-mocha', name: 'Catppuccin Mocha', isDark: true, tokens: { bg:'#1e1e2e', surface:'#181825', raised:'#313244', border:'#45475a', text:'#cdd6f4', muted:'#6c7086', brand:'#cba6f7', timer:'#a6e3a1', onAccent:'#1e1e2e' } },
	{ id: 'synthwave-84', name: "Synthwave '84", isDark: true, tokens: { bg:'#262335', surface:'#1e1a2c', raised:'#34294f', border:'#463465', text:'#ffffff', muted:'#848bbd', brand:'#ff7edb', timer:'#72f1b8', onAccent:'#262335' } },
	{ id: 'github-light', name: 'GitHub Light', isDark: false, tokens: { bg:'#ffffff', surface:'#f6f8fa', raised:'#eaeef2', border:'#d0d7de', text:'#1f2328', muted:'#656d76', brand:'#0969da', timer:'#1a7f37', onAccent:'#ffffff' } },
	{ id: 'oled', name: 'Dark OLED', isDark: true, tokens: { bg:'#000000', surface:'#0a0a0a', raised:'#161616', border:'#242424', text:'#f5f5f5', muted:'#8a8a8a', brand:'#4d9fff', timer:'#30d158', onAccent:'#000000' } },
	{ id: 'colorblind', name: 'Color Blind Friendly', isDark: true, tokens: { bg:'#1b1d21', surface:'#16181b', raised:'#26292e', border:'#363a40', text:'#f0f1f2', muted:'#9198a1', brand:'#56b4e9', timer:'#e69f00', onAccent:'#16181b' } },
];

export const THEME_MAP: Record<string, ThemeDef> =
	THEME_REGISTRY.reduce((acc, t) => { acc[t.id] = t; return acc; }, {} as Record<string, ThemeDef>);

export const THEME_IS_DARK: Record<string, boolean> =
	THEME_REGISTRY.reduce((acc, t) => { acc[t.id] = t.isDark; return acc; }, {} as Record<string, boolean>);
