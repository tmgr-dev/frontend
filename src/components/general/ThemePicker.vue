<template>
	<div class="space-y-5">
		<div class="flex flex-wrap gap-6">
			<button
				v-for="t in cards"
				:key="t.id"
				type="button"
				class="w-[360px] max-w-full rounded-xl border p-3 text-left align-top transition-transform hover:-translate-y-0.5"
				:style="{
					background: t.tokens.bg,
					borderColor: selected === t.id ? t.tokens.brand : t.tokens.border,
					boxShadow:
						selected === t.id ? '0 0 0 2px ' + t.tokens.brand : 'none',
				}"
				@click="choose(t.id)"
			>
				<!-- name + tag -->
				<div class="mb-3 flex items-center gap-2">
					<span
						class="inline-block h-3 w-3 rounded-full"
						:style="{ background: t.tokens.brand }"
					></span>
					<span
						class="text-[15px] font-semibold"
						:style="{ color: t.tokens.text }"
						>{{ t.name }}</span
					>
					<span
						class="font-mono text-[11px] uppercase tracking-wide"
						:style="{ color: t.tokens.muted }"
						>{{ t.tag }}</span
					>
					<span
						v-if="selected === t.id"
						class="ml-auto rounded-full px-2 py-0.5 text-[10px] font-semibold"
						:style="{ background: t.tokens.brand, color: t.tokens.onAccent }"
						>active</span
					>
				</div>

				<!-- swatches -->
				<div class="mb-3 flex gap-1.5">
					<span
						v-for="(c, i) in [
							t.tokens.bg,
							t.tokens.surface,
							t.tokens.border,
							t.tokens.brand,
							t.tokens.timer,
						]"
						:key="i"
						class="h-4 flex-1 rounded"
						:style="{
							background: c,
							outline: '1px solid ' + t.tokens.border,
						}"
					></span>
				</div>

				<!-- live mini preview -->
				<div
					class="flex h-[260px] overflow-hidden rounded-lg"
					:style="{
						border: '1px solid ' + t.tokens.border,
						background: t.tokens.bg,
					}"
				>
					<!-- icon rail -->
					<div
						class="flex w-10 flex-col items-center gap-3 py-3"
						:style="{
							background: t.tokens.bg,
							borderRight: '1px solid ' + t.tokens.border,
						}"
					>
						<span
							class="flex h-5 w-5 items-center justify-center rounded text-[11px] font-bold"
							:style="{ background: t.tokens.brand, color: t.tokens.onAccent }"
							>Y</span
						>
						<span
							class="flex h-4 w-4 items-center justify-center rounded text-[10px]"
							:style="{ background: t.tokens.raised, color: t.tokens.text }"
							>▦</span
						>
						<span class="text-[11px]" :style="{ color: t.tokens.muted }">☰</span>
						<span class="text-[11px]" :style="{ color: t.tokens.muted }">▥</span>
						<span
							class="mt-auto flex h-5 w-5 items-center justify-center rounded-full text-[10px] font-semibold"
							:style="{ background: t.tokens.raised, color: t.tokens.muted }"
							>Y</span
						>
					</div>

					<!-- main -->
					<div class="flex-1 overflow-hidden px-3 py-2.5">
						<div
							class="mb-2 font-mono text-[9px]"
							:style="{ color: t.tokens.muted }"
						>
							TMGR.DEV &nbsp;›&nbsp; Tasks (1955)
						</div>
						<div
							class="mb-2.5 text-center text-[17px] font-bold"
							:style="{ color: t.tokens.text }"
						>
							14618 hours 16 min
						</div>
						<div class="mb-2.5 flex gap-1.5">
							<div
								class="flex-1 rounded-md px-2.5 py-1.5 text-[11px]"
								:style="{
									background: t.tokens.surface,
									border: '1px solid ' + t.tokens.border,
									color: t.tokens.muted,
								}"
							>
								search task
							</div>
							<div
								class="flex h-7 w-7 items-center justify-center rounded-md text-[11px]"
								:style="{
									background: t.tokens.surface,
									border: '1px solid ' + t.tokens.border,
									color: t.tokens.muted,
								}"
							>
								⋮
							</div>
						</div>
						<div
							v-for="row in previewRows"
							:key="row.date"
							class="mb-1.5 rounded-lg px-2.5 py-2"
							:style="{
								background: t.tokens.surface,
								border: '1px solid ' + t.tokens.border,
							}"
						>
							<span
								class="mb-1.5 inline-block rounded px-1.5 py-0.5 font-mono text-[8px] tracking-wide"
								:style="{ background: t.tokens.raised, color: t.tokens.muted }"
								>CURRENT.PROJECT</span
							>
							<div
								class="mb-1.5 font-mono text-[13px]"
								:style="{ color: t.tokens.text }"
							>
								{{ row.date }}
							</div>
							<div class="flex items-center gap-1.5">
								<span class="text-[11px]" :style="{ color: t.tokens.timer }"
									>◷</span
								>
								<span
									class="text-[12px] font-medium"
									:style="{ color: t.tokens.text }"
									>{{ row.time }}</span
								>
								<span
									class="flex h-4 w-4 items-center justify-center rounded text-[9px]"
									:style="{
										background: t.tokens.timer,
										color: t.tokens.onAccent,
									}"
									>▶</span
								>
							</div>
						</div>
					</div>
				</div>

				<!-- default light/dark toggle -->
				<div
					v-if="t.id === 'default'"
					class="mt-3 flex items-center gap-2"
					@click.stop
				>
					<span class="text-xs" :style="{ color: t.tokens.muted }">Mode:</span>
					<span
						class="cursor-pointer rounded-full px-2.5 py-1 text-xs"
						:style="modeStyle('default', t.tokens)"
						@click="chooseScheme('default')"
						>Light</span
					>
					<span
						class="cursor-pointer rounded-full px-2.5 py-1 text-xs"
						:style="modeStyle('dark', t.tokens)"
						@click="chooseScheme('dark')"
						>Dark</span
					>
				</div>
			</button>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, computed } from 'vue';
	import store from '@/store';
	import { THEME_REGISTRY, ThemeDef } from '@/theme/registry';
	import { persistThemeSettings } from '@/actions/tmgr/user';

	const TAGS: Record<string, string> = {
		default: 'canonical',
		dracula: 'purple',
		nord: 'frost',
		'one-dark': 'atom',
		monokai: 'classic',
		'solarized-dark': 'muted',
		'gruvbox-dark': 'retro',
		'tokyo-night': 'night',
		'github-dark': 'default',
		'night-owl': 'deep',
		cobalt2: 'blue',
		palenight: 'material',
		'ayu-dark': 'warm',
		'catppuccin-mocha': 'pastel',
		'synthwave-84': 'neon',
		'github-light': 'light',
		oled: 'true black',
		colorblind: 'okabe-ito',
	};

	const DEFAULT_CARD = {
		id: 'default',
		name: 'Default',
		tag: TAGS.default,
		tokens: {
			bg: 'var(--bg)',
			surface: 'var(--bg-raised)',
			raised: 'var(--bg-sunken)',
			border: 'var(--line-strong-color)',
			text: 'var(--fg)',
			muted: 'var(--fg-muted)',
			brand: 'var(--brand-color)',
			timer: 'var(--timer-color, #30d158)',
			onAccent: 'var(--brand-fg-color)',
		},
	};

	export default defineComponent({
		name: 'ThemePicker',
		setup() {
			const cards = computed(() => [
				DEFAULT_CARD,
				...THEME_REGISTRY.map((t: ThemeDef) => ({
					id: t.id,
					name: t.name,
					tag: TAGS[t.id] || '',
					tokens: t.tokens,
				})),
			]);
			const previewRows = [
				{ date: '2026-07-13', time: '8 hours 34 minutes' },
				{ date: '2026-07-10', time: '14 hours 8 minutes' },
			];
			const selected = computed(() => store.state.theme || 'default');
			const colorScheme = computed(() => store.state.colorScheme || 'default');

			function modeStyle(mode: string, tokens: { brand: string; onAccent: string; muted: string; border: string }) {
				const on = colorScheme.value === mode;
				return {
					background: on ? tokens.brand : 'transparent',
					color: on ? tokens.onAccent : tokens.muted,
					border: '1px solid ' + tokens.border,
				};
			}

			async function persist() {
				try {
					if (store.getters.isLoggedIn) {
						await persistThemeSettings(store.state.theme, store.state.colorScheme);
					}
				} catch (e) {
					console.error('Failed to persist theme', e);
				}
			}
			async function choose(id: string) {
				store.commit('setTheme', id);
				await persist();
			}
			async function chooseScheme(scheme: string) {
				store.commit('setColorScheme', scheme);
				await persist();
			}
			return {
				cards,
				previewRows,
				selected,
				colorScheme,
				modeStyle,
				choose,
				chooseScheme,
			};
		},
	});
</script>
