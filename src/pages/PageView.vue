<template>
	<PageContainer width="wide">
		<div v-if="loading && !page" class="py-16 text-center text-ink-subtle">
			Загрузка...
		</div>
		<div
			v-else-if="loadError"
			role="alert"
			class="py-16 text-center"
			data-testid="page-load-error"
		>
			<p class="text-ink">
				{{
					loadError === 'not_found'
						? 'Страница не найдена'
						: loadError === 'forbidden'
						? 'Страницы отключены или нет доступа'
						: 'Не удалось загрузить страницу'
				}}
			</p>
			<button
				class="mt-2 text-blue-600 underline dark:text-blue-400"
				@click="load"
			>
				Повторить
			</button>
		</div>

		<template v-else-if="page">
			<div
				v-if="updateBanner"
				class="mb-4 flex items-center justify-between gap-3 rounded-lg border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-900 dark:border-amber-700 dark:bg-amber-900/20 dark:text-amber-200"
				role="status"
				data-testid="page-update-banner"
			>
				<span>Страница изменена, версия {{ updateBanner }}</span>
				<button
					type="button"
					class="rounded-md bg-amber-600 px-3 py-1 text-white hover:bg-amber-700"
					@click="refresh"
				>
					Обновить
				</button>
			</div>

			<div
				v-if="conflict && !conflictOpen"
				class="mb-4 flex items-center justify-between gap-3 rounded-lg border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-900 dark:border-red-700 dark:bg-red-900/20 dark:text-red-200"
				role="alert"
			>
				<span>Не сохранено: конфликт версий</span>
				<button
					type="button"
					class="rounded-md bg-red-600 px-3 py-1 text-white hover:bg-red-700"
					@click="conflictOpen = true"
				>
					Разрешить
				</button>
			</div>

			<PageHeader>
				<template #title>
					<input
						v-model="form.title"
						type="text"
						class="w-full min-w-0 border-0 bg-transparent p-0 text-xl font-semibold text-ink placeholder-gray-400 focus:outline-none focus:ring-0 md:text-2xl"
						placeholder="Без названия"
						maxlength="255"
						data-testid="page-title-input"
						@blur="normalizeTitle"
						@keydown.enter.prevent="($event.target as HTMLInputElement).blur()"
					/>
				</template>
				<template #actions>
					<span
						class="text-xs text-ink-subtle"
						data-testid="page-save-status"
						role="status"
					>
						{{ saveStatus }}
					</span>
					<button
						v-if="saveError"
						type="button"
						class="text-xs text-red-600 underline dark:text-red-400"
						@click="retrySave"
					>
						Повторить
					</button>
					<router-link
						:to="`/${workspaceCode}/pages/${page.slug}/versions`"
						class="inline-flex items-center gap-1 rounded-md border border-gray-300 px-2.5 py-1 text-sm text-gray-700 hover:bg-gray-50 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-800"
					>
						<History class="h-4 w-4" />
						История
					</router-link>
				</template>
			</PageHeader>

			<PageProperties
				:page="page"
				:properties="form.properties"
				:directory="directory"
				:errors="propertyErrors"
				@update="onPropertiesUpdate"
				@navigate="onNavigate"
			/>

			<div class="grid gap-8 lg:grid-cols-[minmax(0,1fr)_16rem]">
				<div ref="contentRef" class="min-w-0" data-testid="page-content">
					<template
						v-for="(segment, index) in segments"
						:key="`${editorKey}-${index}`"
					>
						<PageEditor
							v-if="segment.kind === 'free'"
							:model-value="segment.text"
							:directory="directory"
							placeholder="Начните писать... @ — ссылка, [[ — страница"
							@change="(md: string, dirty: boolean) => onFreeChange(index, md, dirty)"
							@navigate="onNavigate"
						/>
						<PageSection
							v-else
							:section="segment"
							:directory="directory"
							:saving="sectionSaving"
							@save="onSectionSave"
							@navigate="onNavigate"
						/>
					</template>
				</div>

				<PageSidePanel
					:toc="toc"
					:backlinks="page.backlinks || []"
					:versions="versions"
					:workspace-code="workspaceCode"
					:slug="page.slug"
					@toc="scrollToHeading"
				/>
			</div>

			<PageConflictDialog
				v-if="conflict && conflictOpen"
				:theirs="conflict"
				:draft-body="form.body"
				:comparing="comparing"
				@choose="onConflictChoose"
				@dismiss="conflictOpen = false"
			/>
		</template>
	</PageContainer>
</template>

<script lang="ts">
	import {
		getPage,
		getPageVersions,
		type Page,
		PageConflictError,
		type PageVersion,
		setPageSection,
		updatePage,
	} from '@/actions/tmgr/pages';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import PageConflictDialog from '@/components/pages/PageConflictDialog.vue';
	import PageEditor from '@/components/pages/PageEditor.vue';
	import PageProperties from '@/components/pages/PageProperties.vue';
	import PageSection from '@/components/pages/PageSection.vue';
	import PageSidePanel from '@/components/pages/PageSidePanel.vue';
	import { useTmgrDirectory } from '@/components/pages/useTmgrDirectory';
	import { useDebouncedAutoSave } from '@/composable/useDebouncedAutoSave';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import store from '@/store';
	import type { RootState } from '@/types/store';
	import {
		changedFields,
		type ConflictChoice,
		type PageDraft,
		resolveConflict,
	} from '@/utils/pages/conflict';
	import {
		parsePropertyErrors,
		type PropertyErrors,
		validateProperties,
	} from '@/utils/pages/properties';
	import {
		type PageUpdatedEvent,
		shouldShowUpdateBanner,
	} from '@/utils/pages/realtime';
	import {
		applyFreeEdit,
		joinSegments,
		padSegments,
		replaceSectionInner,
		sectionInner,
		type Segment,
		splitBody,
	} from '@/utils/pages/sections';
	import type { ParsedTmgrUrl } from '@/utils/pages/tmgrLinks';
	import { extractToc, type TocEntry } from '@/utils/pages/toc';
	import { isSaveHotkey } from '@/utils/saveHotkey';
	import { History } from 'lucide-vue-next';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
		watch,
	} from 'vue';
	import { useRoute, useRouter } from 'vue-router';

	const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value));

	export default defineComponent({
		name: 'PageView',
		components: {
			History,
			PageConflictDialog,
			PageContainer,
			PageEditor,
			PageHeader,
			PageProperties,
			PageSection,
			PageSidePanel,
		},
		setup() {
			const route = useRoute();
			const router = useRouter();
			const workspaceCode = computed(() => String(route.params.workspace_code));
			const slug = computed(() => String(route.params.slug));
			const directory = useTmgrDirectory(() => workspaceCode.value);

			const page = ref<Page | null>(null);
			const loading = ref(false);
			const loadError = ref<'not_found' | 'forbidden' | 'error' | null>(null);
			const form = ref<PageDraft>({ title: '', body: '', properties: {} });
			const savedState = ref<PageDraft>({
				title: '',
				body: '',
				properties: {},
			});
			const segments = ref<Segment[]>([]);
			let originals: Segment[] = [];
			const editorKey = ref(0);
			const contentRef = ref<HTMLElement | null>(null);
			const versions = ref<PageVersion[]>([]);
			const conflict = ref<Page | null>(null);
			const conflictOpen = ref(false);
			const comparing = ref(false);
			const updateBanner = ref<number | null>(null);
			const saveError = ref(false);
			const sectionSaving = ref(false);
			const serverPropertyErrors = ref<PropertyErrors>({});
			const ownVersions = new Set<number>();
			let blockedProperties: string | null = null;
			let applying = false;
			let chain: Promise<unknown> = Promise.resolve();
			let loadSeq = 0;

			const runExclusive = <T>(task: () => Promise<T>): Promise<T> => {
				const next = chain.then(task, task);
				chain = next.catch(() => undefined);
				return next;
			};

			const clientPropertyErrors = computed<PropertyErrors>(() =>
				page.value
					? validateProperties(page.value.type, form.value.properties)
					: {},
			);
			const propertyErrors = computed<PropertyErrors>(() => ({
				...serverPropertyErrors.value,
				...clientPropertyErrors.value,
			}));

			const toc = computed<TocEntry[]>(() => extractToc(form.value.body));

			const mergeSaved = (updated: Page) => {
				if (!page.value) return;
				ownVersions.add(updated.version);
				page.value = {
					...page.value,
					version: updated.version,
					updated_at: updated.updated_at,
					updated_by: updated.updated_by,
					title: updated.title,
					slug: updated.slug,
					sections: updated.sections ?? page.value.sections,
					backlinks: updated.backlinks ?? page.value.backlinks,
				};
				void loadVersions();
			};

			const onConflict = (current: Page) => {
				cancel();
				comparing.value = false;
				conflict.value = current;
				conflictOpen.value = true;
			};

			const sendUpdate = (snapshot: PageDraft) =>
				runExclusive(async () => {
					if (!page.value || conflict.value) return;
					const fields = changedFields(snapshot, savedState.value);
					if (
						fields.properties &&
						(Object.keys(clientPropertyErrors.value).length ||
							JSON.stringify(fields.properties) === blockedProperties)
					) {
						delete fields.properties;
					}
					if (!Object.keys(fields).length) return;
					try {
						const updated = await updatePage(page.value.id, {
							version: page.value.version,
							...fields,
						});
						savedState.value = clone({ ...savedState.value, ...fields });
						if (fields.properties) {
							serverPropertyErrors.value = {};
							blockedProperties = null;
						}
						saveError.value = false;
						mergeSaved(updated);
					} catch (error) {
						const invalid = fields.properties
							? parsePropertyErrors(error)
							: null;
						if (error instanceof PageConflictError) {
							onConflict(error.current);
						} else if (invalid) {
							serverPropertyErrors.value = invalid;
							blockedProperties = JSON.stringify(fields.properties);
							saveError.value = false;
							const { properties: _blocked, ...rest } = fields;
							if (Object.keys(rest).length) {
								try {
									const updated = await updatePage(page.value.id, {
										version: page.value.version,
										...rest,
									});
									savedState.value = clone({ ...savedState.value, ...rest });
									mergeSaved(updated);
								} catch (retryError) {
									if (retryError instanceof PageConflictError) {
										onConflict(retryError.current);
									} else {
										saveError.value = true;
									}
								}
							}
						} else {
							saveError.value = true;
						}
					}
				});

			const [isSaving, cancel, flush, saveNow, hasPending] =
				useDebouncedAutoSave<PageDraft>({
					formRef: form,
					onSave: sendUpdate,
					fieldsToWatch: ['title', 'body', 'properties'],
					delay: 2000,
					enabled: () =>
						!applying &&
						!conflict.value &&
						!sectionSaving.value &&
						!!page.value,
				});

			const saveStatus = computed(() => {
				if (saveError.value) return 'Не удалось сохранить';
				if (conflict.value) return 'Конфликт версий';
				if (isSaving.value) return 'Сохранение...';
				if (hasPending.value) return 'Есть несохранённые изменения';
				return page.value ? `Сохранено, версия ${page.value.version}` : '';
			});

			const retrySave = () => {
				saveError.value = false;
				void flush(true);
			};

			async function loadVersions() {
				if (!page.value) return;
				const id = page.value.id;
				try {
					const list = await getPageVersions(id);
					if (page.value?.id === id) versions.value = list.slice(0, 5);
				} catch {
					versions.value = [];
				}
			}

			const adopt = (next: Page) => {
				applying = true;
				cancel();
				page.value = next;
				form.value = {
					title: next.title,
					body: next.body,
					properties: clone(next.properties ?? {}),
				};
				savedState.value = clone(form.value);
				originals = padSegments(splitBody(next.body));
				segments.value = clone(originals);
				editorKey.value++;
				conflict.value = null;
				conflictOpen.value = false;
				comparing.value = false;
				updateBanner.value = null;
				saveError.value = false;
				serverPropertyErrors.value = {};
				blockedProperties = null;
				ownVersions.clear();
				ownVersions.add(next.version);
				setDocumentTitle(next.title);
				applying = false;
				void loadVersions();
			};

			async function load() {
				const seq = ++loadSeq;
				loading.value = true;
				loadError.value = null;
				try {
					const loaded = await getPage(slug.value);
					if (seq !== loadSeq) return;
					adopt(loaded);
				} catch (error: any) {
					if (seq !== loadSeq) return;
					const status = error?.response?.status;
					loadError.value =
						status === 404
							? 'not_found'
							: status === 403
							? 'forbidden'
							: 'error';
					page.value = null;
				} finally {
					if (seq === loadSeq) loading.value = false;
				}
			}

			const refresh = async () => {
				if (!page.value) return;
				try {
					const current = await getPage(page.value.id);
					const dirty =
						JSON.stringify(changedFields(form.value, savedState.value)) !==
						'{}';
					if (dirty) {
						cancel();
						comparing.value = false;
						conflict.value = current;
						conflictOpen.value = true;
					} else {
						adopt(current);
					}
				} catch {
					return;
				}
			};

			const onFreeChange = (
				index: number,
				markdown: string,
				dirty: boolean,
			) => {
				const next = dirty
					? applyFreeEdit(segments.value, index, markdown)
					: segments.value.map((segment, i) =>
							i === index ? originals[index] : segment,
					  );
				segments.value = next;
				form.value.body = joinSegments(next);
			};

			const onPropertiesUpdate = (properties: Record<string, any>) => {
				serverPropertyErrors.value = {};
				form.value.properties = properties;
			};

			const normalizeTitle = () => {
				if (!form.value.title.trim()) {
					form.value.title = savedState.value.title;
				}
			};

			const onSectionSave = async (payload: {
				id: string;
				markdown: string;
				done: (ok: boolean) => void;
			}) => {
				if (!page.value || conflict.value) {
					payload.done(false);
					return;
				}
				await flush();
				if (conflict.value || !page.value) {
					payload.done(false);
					return;
				}
				sectionSaving.value = true;
				let ok = false;
				try {
					await runExclusive(async () => {
						try {
							const updated = await setPageSection(
								page.value!.id,
								payload.id,
								payload.markdown,
							);
							const inner =
								sectionInner(updated.body, payload.id) ?? payload.markdown;
							segments.value = segments.value.map((segment) =>
								segment.kind === 'section' && segment.id === payload.id
									? { ...segment, inner }
									: segment,
							);
							originals = originals.map((segment) =>
								segment.kind === 'section' && segment.id === payload.id
									? { ...segment, inner }
									: segment,
							);
							applying = true;
							form.value.body = replaceSectionInner(
								form.value.body,
								payload.id,
								inner,
							);
							savedState.value = {
								...savedState.value,
								body: replaceSectionInner(
									savedState.value.body,
									payload.id,
									inner,
								),
							};
							applying = false;
							mergeSaved(updated);
							ok = true;
						} catch (error) {
							if (error instanceof PageConflictError) onConflict(error.current);
							else saveError.value = true;
						}
					});
				} finally {
					sectionSaving.value = false;
				}
				payload.done(ok);
				if (
					ok &&
					changedFields(form.value, savedState.value).body !== undefined
				) {
					void saveNow(clone(form.value));
				}
			};

			const onConflictChoose = async (choice: ConflictChoice) => {
				if (!conflict.value || !page.value) return;
				const theirs = conflict.value;
				const outcome = resolveConflict(choice, {
					draft: form.value,
					base: savedState.value,
					theirs,
				});
				if (outcome.kind === 'compare') {
					comparing.value = true;
					return;
				}
				if (outcome.kind === 'adopt') {
					adopt(theirs);
					return;
				}
				try {
					const updated = await runExclusive(() =>
						updatePage(theirs.id, outcome.payload),
					);
					adopt(updated);
				} catch (error) {
					if (error instanceof PageConflictError) {
						conflict.value = error.current;
						comparing.value = false;
					} else {
						saveError.value = true;
					}
				}
			};

			const onNavigate = async (parsed: ParsedTmgrUrl) => {
				if (parsed.form === 'storage' && parsed.kind === 'file') return;
				let target = directory.pathFor(parsed);
				if (!target && parsed.form === 'storage') {
					await directory.ensure([{ kind: parsed.kind, id: parsed.id }]);
					target = directory.pathFor(parsed);
				}
				if (target) void router.push(target);
			};

			const scrollToHeading = (entry: TocEntry) => {
				const headings = Array.from(
					contentRef.value?.querySelectorAll('h2') ?? [],
				).filter((h) => (h.textContent || '').trim() === entry.text);
				headings[entry.occurrence]?.scrollIntoView({
					behavior: 'smooth',
					block: 'start',
				});
			};

			const onRealtime = (event: PageUpdatedEvent) => {
				if (!page.value) return;
				const version = shouldShowUpdateBanner({
					pageId: page.value.id,
					loadedVersion: page.value.version,
					ownVersions,
					event,
				});
				if (version) updateBanner.value = version;
			};

			watch(
				() => (store.state as RootState).pagesEvent?.seq,
				() => {
					const event = (store.state as RootState).pagesEvent;
					if (event?.type === 'page.updated') onRealtime(event);
				},
			);

			const onKeydown = (event: KeyboardEvent) => {
				if (!isSaveHotkey(event)) return;
				event.preventDefault();
				void flush(true);
			};

			onMounted(() => {
				void load();
				window.addEventListener('keydown', onKeydown);
			});

			onBeforeUnmount(() => {
				window.removeEventListener('keydown', onKeydown);
			});

			watch(slug, (next, previous) => {
				if (next !== previous && page.value?.slug !== next) {
					void flush().then(load);
				}
			});

			return {
				workspaceCode,
				directory,
				page,
				loading,
				loadError,
				load,
				form,
				segments,
				editorKey,
				contentRef,
				versions,
				toc,
				conflict,
				conflictOpen,
				comparing,
				updateBanner,
				saveError,
				saveStatus,
				sectionSaving,
				refresh,
				retrySave,
				normalizeTitle,
				onFreeChange,
				propertyErrors,
				onPropertiesUpdate,
				onSectionSave,
				onConflictChoose,
				onNavigate,
				scrollToHeading,
			};
		},
	});
</script>
