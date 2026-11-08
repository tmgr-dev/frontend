<template>
	<div
		class="mx-auto flex w-full max-w-7xl min-w-0 flex-1 gap-8 px-4 pt-4 pb-16 md:px-6 md:pt-6"
		data-testid="docs-page"
	>
		<nav :aria-label="ui.docs" class="hidden w-56 shrink-0 lg:block">
			<div
				class="sticky top-[calc(var(--titlebar-h,0px)+1rem)] max-h-[calc(100svh-2rem)] overflow-y-auto pb-4"
			>
				<router-link
					:to="docPath(lang, '')"
					class="mb-4 block px-2 text-base font-semibold text-ink"
				>
					{{ ui.docs }}
				</router-link>
				<DocsNav :nav="nav" :lang="lang" :slug="slug" />
			</div>
		</nav>

		<article class="min-w-0 flex-1" :lang="shownLang">
			<div class="mb-6 flex items-center gap-2">
				<button
					type="button"
					class="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-input bg-background text-ink hover:bg-muted lg:hidden"
					:aria-label="ui.menu"
					data-testid="docs-menu-button"
					@click="drawerOpen = true"
				>
					<Menu class="h-4 w-4" aria-hidden="true" />
				</button>
				<button
					type="button"
					class="inline-flex h-9 min-w-0 flex-1 items-center gap-2 rounded-md border border-input bg-background px-3 text-sm text-ink-subtle hover:bg-muted hover:text-ink sm:max-w-xs"
					:aria-label="ui.search"
					aria-keyshortcuts="/"
					data-testid="docs-search-button"
					@click="searchOpen = true"
				>
					<Search class="h-4 w-4 shrink-0" aria-hidden="true" />
					<span class="truncate">{{ ui.searchPlaceholder }}</span>
					<kbd
						class="ml-auto hidden rounded border border-border bg-muted px-1.5 font-mono text-xs text-ink-subtle sm:inline"
						aria-hidden="true"
						>/</kbd
					>
				</button>
				<a
					:href="otherLangHref"
					:hreflang="otherLang"
					:lang="otherLang"
					:aria-label="ui.otherLanguageLabel"
					class="ml-auto inline-flex h-9 shrink-0 items-center gap-1.5 rounded-md border border-input bg-background px-3 text-sm text-ink hover:bg-muted"
					data-testid="docs-lang-switch"
					@click.prevent="switchLanguage"
				>
					<Languages class="h-4 w-4" aria-hidden="true" />
					{{ ui.otherLanguage }}
				</a>
			</div>

			<template v-if="resolved && rendered">
				<p
					v-if="resolved.fallback"
					class="mb-6 rounded-md border border-border bg-muted px-3 py-2 text-sm text-ink-subtle"
					role="note"
					:lang="lang"
					data-testid="docs-fallback-note"
				>
					{{ ui.fallbackNote }}
				</p>

				<header class="mb-8">
					<h1
						tabindex="-1"
						class="text-2xl font-semibold tracking-tight text-ink outline-none sm:text-3xl"
						data-docs-title
					>
						{{ resolved.page.title }}
					</h1>
					<p
						v-if="resolved.page.description"
						class="mt-2 text-base text-ink-subtle"
					>
						{{ resolved.page.description }}
					</p>
				</header>

				<div
					v-if="videoUrl"
					class="mb-8 aspect-video overflow-hidden rounded-lg border border-border bg-muted"
					data-testid="docs-video"
				>
					<iframe
						:src="videoUrl"
						:title="`${ui.video}: ${resolved.page.title}`"
						class="h-full w-full"
						allow="
							accelerometer;
							clipboard-write;
							encrypted-media;
							gyroscope;
							picture-in-picture;
							web-share;
						"
						referrerpolicy="strict-origin-when-cross-origin"
						loading="lazy"
						allowfullscreen
					></iframe>
				</div>

				<div data-testid="docs-content" @click="onContentClick">
					<template v-for="(segment, index) in rendered.segments" :key="index">
						<div
							v-if="segment.type === 'html'"
							class="docs-prose"
							v-html="segment.html"
						></div>
						<div v-else-if="componentFor(segment.name)" class="my-6">
							<component
								:is="componentFor(segment.name)!.component"
								v-bind="componentFor(segment.name)!.props"
							/>
						</div>
					</template>
				</div>

				<nav
					v-if="pager.prev || pager.next"
					:aria-label="`${ui.previous} / ${ui.next}`"
					class="mt-12 grid gap-3 border-t border-border pt-6 sm:grid-cols-2"
				>
					<router-link
						v-if="pager.prev"
						:to="docPath(lang, pager.prev.slug)"
						class="rounded-md border border-border px-4 py-3 hover:bg-muted"
						data-testid="docs-prev"
					>
						<span class="flex items-center gap-1 text-xs text-ink-subtle">
							<ChevronLeft class="h-3.5 w-3.5" aria-hidden="true" />
							{{ ui.previous }}
						</span>
						<span class="mt-1 block text-sm font-medium text-ink">
							{{ pager.prev.title }}
						</span>
					</router-link>
					<router-link
						v-if="pager.next"
						:to="docPath(lang, pager.next.slug)"
						class="rounded-md border border-border px-4 py-3 text-right hover:bg-muted sm:col-start-2"
						data-testid="docs-next"
					>
						<span
							class="flex items-center justify-end gap-1 text-xs text-ink-subtle"
						>
							{{ ui.next }}
							<ChevronRight class="h-3.5 w-3.5" aria-hidden="true" />
						</span>
						<span class="mt-1 block text-sm font-medium text-ink">
							{{ pager.next.title }}
						</span>
					</router-link>
				</nav>
			</template>

			<div v-else class="py-12" data-testid="docs-not-found">
				<h1
					tabindex="-1"
					class="text-2xl font-semibold tracking-tight text-ink outline-none"
					data-docs-title
				>
					{{ ui.notFoundTitle }}
				</h1>
				<p class="mt-2 text-ink-subtle">{{ ui.notFoundText }}</p>
				<router-link
					:to="docPath(lang, '')"
					class="mt-4 inline-block text-primary underline underline-offset-2"
				>
					{{ ui.backToIndex }}
				</router-link>
			</div>
		</article>

		<aside
			v-if="toc.length"
			:aria-label="ui.onThisPage"
			class="hidden w-52 shrink-0 xl:block"
		>
			<div
				class="sticky top-[calc(var(--titlebar-h,0px)+1rem)] max-h-[calc(100svh-2rem)] overflow-y-auto text-sm"
			>
				<p
					class="mb-2 text-xs font-semibold tracking-wide text-ink-subtle uppercase"
				>
					{{ ui.onThisPage }}
				</p>
				<ul class="space-y-1" data-testid="docs-toc">
					<li
						v-for="heading in toc"
						:key="heading.id"
						:class="heading.depth === 3 ? 'pl-3' : ''"
					>
						<a
							:href="`#${heading.id}`"
							class="block py-0.5 text-ink-subtle hover:text-ink"
							@click="onAnchorClick($event, `#${heading.id}`)"
						>
							{{ heading.text }}
						</a>
					</li>
				</ul>
			</div>
		</aside>

		<Sheet v-model:open="drawerOpen">
			<SheetContent side="left" class="overflow-y-auto">
				<SheetTitle class="mb-4 text-base">{{ ui.docs }}</SheetTitle>
				<SheetDescription class="sr-only">{{ ui.menu }}</SheetDescription>
				<nav :aria-label="ui.docs">
					<DocsNav :nav="nav" :lang="lang" :slug="slug" />
				</nav>
			</SheetContent>
		</Sheet>

		<DocsSearch
			v-model:open="searchOpen"
			:entries="searchEntries"
			:ui="ui"
			@select="onSearchSelect"
		/>

		<span class="sr-only" role="status" aria-live="polite">{{
			copyStatus
		}}</span>
	</div>
</template>

<script lang="ts">
	import AgentConnectTabs from '@/components/agents/AgentConnectTabs.vue';
	import AiAgentsOverview from '@/components/agents/AiAgentsOverview.vue';
	import DocsNav from '@/components/docs/DocsNav.vue';
	import DocsSearch from '@/components/docs/DocsSearch.vue';
	import {
		Sheet,
		SheetContent,
		SheetDescription,
		SheetTitle,
	} from '@/components/ui/sheet';
	import { toast } from '@/components/ui/toast';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { DOCS_PAGES } from '@/docs/loader';
	import { currentLocale, onLocaleChange } from '@/i18n';
	import {
		buildNav,
		buildSearchIndex,
		docPath,
		DOCS_COMPONENTS,
		DOCS_UI,
		docsHead,
		parseDocsPath,
		prevNext,
		resolveDoc,
		youtubeEmbedUrl,
		type DocsSearchResult,
	} from '@/utils/docs';
	import { renderDoc } from '@/utils/docsMarkdown';
	import { shouldIgnoreNavigationTarget } from '@/utils/listKeyboardNavigation';
	import { resolveMcpUrl } from '@/utils/mcpConnectSnippets';
	import {
		ChevronLeft,
		ChevronRight,
		Languages,
		Menu,
		Search,
	} from 'lucide-vue-next';
	import {
		computed,
		defineComponent,
		markRaw,
		nextTick,
		onBeforeUnmount,
		onMounted,
		ref,
		watch,
		type Component,
	} from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	const HEAD_ATTR = 'data-docs-head';

	const clearDocsHead = () =>
		document.head
			.querySelectorAll(`[${HEAD_ATTR}]`)
			.forEach((el) => el.remove());

	const REGISTRY: Record<
		(typeof DOCS_COMPONENTS)[number],
		{ component: Component; props: Record<string, unknown> }
	> = {
		AiAgentsOverview: {
			component: markRaw(AiAgentsOverview),
			props: { showSetupLinks: false },
		},
		AgentConnectTabs: { component: markRaw(AgentConnectTabs), props: {} },
	};

	export default defineComponent({
		name: 'DocsPage',
		components: {
			ChevronLeft,
			ChevronRight,
			DocsNav,
			DocsSearch,
			Languages,
			Menu,
			Search,
			Sheet,
			SheetContent,
			SheetDescription,
			SheetTitle,
		},
		setup() {
			const route = useRoute();
			const router = useRouter();
			const store = useStore();

			const location = computed(() => parseDocsPath(route.path));
			const lang = computed(() => location.value.lang);
			const slug = computed(() => location.value.slug);
			const ui = computed(() => DOCS_UI[lang.value]);
			const otherLang = computed(() => (lang.value === 'en' ? 'ru' : 'en'));
			const otherLangHref = computed(() =>
				docPath(otherLang.value, slug.value),
			);

			const nav = computed(() => buildNav(DOCS_PAGES, lang.value));
			const resolved = computed(() =>
				resolveDoc(DOCS_PAGES, lang.value, slug.value),
			);
			const shownLang = computed(() => resolved.value?.page.lang ?? lang.value);
			const pager = computed(() => prevNext(nav.value, slug.value));
			const videoUrl = computed(() =>
				youtubeEmbedUrl(resolved.value?.page.video),
			);

			const mcpUrl = resolveMcpUrl(
				import.meta.env.VITE_API_BASE_URL,
				window.location.origin,
			);
			const rendered = computed(() =>
				resolved.value
					? renderDoc(resolved.value.page.body, {
							copyLabel: DOCS_UI[shownLang.value].copy,
							vars: { mcpUrl },
						})
					: null,
			);
			const toc = computed(() =>
				(rendered.value?.headings ?? []).filter(
					(h) => h.depth === 2 || h.depth === 3,
				),
			);
			const searchEntries = computed(() =>
				buildSearchIndex(nav.value.flatMap((s) => s.items)),
			);

			const drawerOpen = ref(false);
			const searchOpen = ref(false);
			const copyStatus = ref('');

			const componentFor = (name: string) =>
				REGISTRY[name as keyof typeof REGISTRY] ?? null;

			watch(
				() => resolved.value?.page.title ?? ui.value.notFoundTitle,
				(title) => {
					setDocumentTitle(title);
					store.commit('setMetaTitle', title);
				},
				{ immediate: true },
			);

			const applyHead = () => {
				clearDocsHead();
				document.documentElement.lang = lang.value;
				for (const { tag, attrs } of docsHead(
					DOCS_PAGES,
					lang.value,
					slug.value,
				)?.tags ?? [
					{ tag: 'meta', attrs: { name: 'robots', content: 'noindex' } },
				]) {
					const el = document.createElement(tag);
					Object.entries(attrs).forEach(([key, value]) =>
						el.setAttribute(key, value),
					);
					el.setAttribute(HEAD_ATTR, '');
					document.head.appendChild(el);
				}
			};
			watch(location, applyHead, { immediate: true });
			const offLocaleChange = onLocaleChange(() => {
				document.documentElement.lang = lang.value;
			});

			const scrollToId = (id: string, focus: boolean) => {
				const target = id ? document.getElementById(id) : null;
				if (!target) return false;
				target.scrollIntoView({ block: 'start' });
				if (focus) target.focus({ preventScroll: true });
				return true;
			};

			const scrollToHash = (hash: string, focus = false) => {
				let id = hash.replace(/^#/, '');
				try {
					id = decodeURIComponent(id);
				} catch {
					return;
				}
				scrollToId(id, focus);
			};

			const go = (href: string) => {
				const url = new URL(href, window.location.href);
				const samePage =
					url.origin === window.location.origin &&
					url.pathname.replace(/\/$/, '') === route.path.replace(/\/$/, '') &&
					url.search === window.location.search;
				if (!samePage) {
					router.push(`${url.pathname}${url.search}${url.hash}`);
					return;
				}
				if (url.hash) {
					const target = router.resolve(
						`${url.pathname}${url.search}${url.hash}`,
					);
					history.replaceState(
						{ ...history.state, current: target.fullPath },
						'',
						target.href,
					);
					scrollToHash(url.hash, true);
				} else window.scrollTo({ top: 0 });
			};

			const isPlainClick = (event: MouseEvent) =>
				event.button === 0 &&
				!event.metaKey &&
				!event.ctrlKey &&
				!event.shiftKey &&
				!event.altKey &&
				!event.defaultPrevented;

			const onAnchorClick = (event: MouseEvent, href: string) => {
				if (!isPlainClick(event)) return;
				event.preventDefault();
				go(href);
			};

			const copyTimers = new Map<HTMLElement, ReturnType<typeof setTimeout>>();
			const copy = async (button: HTMLElement) => {
				const code =
					button.parentElement?.querySelector('code')?.textContent ?? '';
				const labels = DOCS_UI[shownLang.value];
				try {
					await navigator.clipboard.writeText(code);
					button.textContent = labels.copied;
					copyStatus.value = labels.copied;
					clearTimeout(copyTimers.get(button));
					copyTimers.set(
						button,
						setTimeout(() => {
							copyTimers.delete(button);
							button.textContent = labels.copy;
							if (!copyTimers.size) copyStatus.value = '';
						}, 2000),
					);
				} catch {
					toast({ title: labels.copyFailed, variant: 'destructive' });
				}
			};

			const onContentClick = (event: MouseEvent) => {
				const target = event.target as HTMLElement | null;
				const button = target?.closest<HTMLElement>('[data-docs-copy]');
				if (button) {
					void copy(button);
					return;
				}
				const link = target?.closest<HTMLAnchorElement>('a[data-docs-link]');
				if (link) onAnchorClick(event, link.getAttribute('href') ?? '');
			};

			const switchLanguage = () =>
				router.push(`${otherLangHref.value}${window.location.hash}`);

			const onSearchSelect = (result: DocsSearchResult) =>
				nextTick(() =>
					go(
						`${docPath(lang.value, result.slug)}${
							result.anchor ? `#${result.anchor}` : ''
						}`,
					),
				);

			const onKeydown = (event: KeyboardEvent) => {
				if (
					event.key !== '/' ||
					event.metaKey ||
					event.ctrlKey ||
					event.altKey ||
					event.defaultPrevented ||
					searchOpen.value ||
					shouldIgnoreNavigationTarget(event.target) ||
					document.querySelector('[role="dialog"][data-state="open"]')
				)
					return;
				event.preventDefault();
				searchOpen.value = true;
			};

			onMounted(() => {
				window.addEventListener('keydown', onKeydown);
				nextTick(() =>
					requestAnimationFrame(() => {
						const lost =
							!document.activeElement ||
							document.activeElement === document.body;
						if (route.hash) scrollToHash(route.hash, lost);
						else if (lost)
							document
								.querySelector<HTMLElement>('[data-docs-title]')
								?.focus({ preventScroll: true });
					}),
				);
			});

			onBeforeUnmount(() => {
				offLocaleChange();
				if (!/^\/docs(\/|$)/.test(router.currentRoute.value.path)) {
					clearDocsHead();
					document.documentElement.lang = currentLocale();
				}
				window.removeEventListener('keydown', onKeydown);
				copyTimers.forEach((timer) => clearTimeout(timer));
			});

			return {
				lang,
				slug,
				ui,
				otherLang,
				otherLangHref,
				nav,
				resolved,
				shownLang,
				pager,
				videoUrl,
				rendered,
				toc,
				searchEntries,
				drawerOpen,
				searchOpen,
				copyStatus,
				docPath,
				componentFor,
				onAnchorClick,
				onContentClick,
				switchLanguage,
				onSearchSelect,
			};
		},
	});
</script>
