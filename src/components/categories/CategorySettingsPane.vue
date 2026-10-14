<template>
	<section
		class="flex min-w-0 flex-1 flex-col lg:min-h-0"
		:aria-label="heading"
	>
		<div
			v-if="!isCreate"
			class="-mx-4 -mt-4 flex items-center gap-1 border-b border-line px-3 py-2.5 lg:hidden"
		>
			<Button
				type="button"
				variant="ghost"
				class="h-8 gap-1 px-1.5 text-ink hover:bg-surface-hover"
				aria-label="Back to categories"
				@click="$emit('close')"
			>
				<ChevronLeft class="!size-[18px]" />
				Categories
			</Button>
			<span class="flex-1" />
			<Button
				v-if="!isCreate && tasksLink"
				as-child
				variant="outline"
				size="sm"
				class="h-[30px] border-line-strong bg-surface text-xs text-ink"
			>
				<router-link :to="tasksLink">Tasks →</router-link>
			</Button>
		</div>

		<div
			class="flex items-start gap-2 pt-4 lg:border-b lg:border-line lg:px-7 lg:pb-4 lg:pt-[18px]"
		>
			<div class="flex min-w-0 flex-1 flex-col gap-0.5">
				<span class="truncate text-xs text-ink-subtle">{{ crumb }}</span>
				<h2
					class="truncate text-[22px] font-semibold leading-snug text-ink lg:text-xl"
				>
					{{ heading }}
				</h2>
			</div>
			<template v-if="!isCreate">
				<Button
					v-if="tasksLink"
					as-child
					variant="outline"
					class="hidden h-8 border-line-strong bg-surface px-3 text-[13px] text-ink lg:inline-flex"
				>
					<router-link :to="tasksLink">Tasks →</router-link>
				</Button>
				<Button
					type="button"
					variant="outline"
					class="hidden h-8 border-line-strong bg-surface px-3 text-[13px] text-ink lg:inline-flex"
					@click="$emit('add-sub')"
				>
					+ Subcategory
				</Button>
			</template>
		</div>

		<div class="pb-6 pt-1 lg:min-h-0 lg:flex-1 lg:overflow-y-auto lg:px-7">
			<form :id="formId" novalidate @submit.prevent="submit">
				<SectionHead title="General" class="max-lg:hidden" />
				<SettingRow label="Name">
					<div ref="nameField">
						<TextField
							v-model="form.title"
							placeholder="Name"
							input-class="h-9 text-sm"
							:class="inputSkin"
							:errors="errors.title"
							:show-error-in-tooltip="false"
						/>
					</div>
				</SettingRow>
				<SettingRow label="Code" hint="Letters and digits" width="md">
					<TextField
						:model-value="form.code"
						placeholder="e.g., PROJ, FEAT, BUG"
						input-class="h-9 font-mono text-[13px]"
						:class="inputSkin"
						:errors="errors.code"
						:show-error-in-tooltip="false"
						@update:model-value="form.code = sanitizeCategoryCode($event)"
					/>
				</SettingRow>
				<SettingRow label="Parent" width="lg">
					<Select v-model="form.parent">
						<SelectTrigger :class="fieldClass" aria-label="Parent">
							<SelectValue placeholder="No parent" />
						</SelectTrigger>
						<SelectContent>
							<SelectItem value="none">No parent</SelectItem>
							<SelectItem
								v-for="option in parentChoices"
								:key="option.category.id"
								:value="String(option.category.id)"
								:style="{ paddingLeft: `${32 + option.depth * 14}px` }"
							>
								{{ option.category.title }}
							</SelectItem>
						</SelectContent>
					</Select>
					<p
						v-if="errors.project_category_id"
						class="mt-1 text-sm text-red-500"
					>
						{{ errors.project_category_id[0] }}
					</p>
				</SettingRow>

				<template v-if="catalogueState !== 'unavailable'">
					<SectionHead
						title="New tasks"
						hint="Defaults for tasks created in this category"
						class="max-lg:pt-2"
					/>
					<div v-if="catalogueState === 'loading'" class="space-y-3 py-3">
						<Skeleton v-for="n in 3" :key="n" class="h-9 w-full" />
					</div>
					<template v-else>
						<SettingRow
							v-if="isCreate && copyDefault"
							label="Copy settings from"
							tag="Proposal"
							width="lg"
						>
							<Select :model-value="copyFrom" @update:model-value="applyCopy">
								<SelectTrigger
									:class="fieldClass"
									aria-label="Copy settings from"
								>
									<SelectValue />
								</SelectTrigger>
								<SelectContent>
									<SelectItem value="none">Nothing</SelectItem>
									<SelectItem
										v-for="option in copyChoices"
										:key="option.id"
										:value="String(option.id)"
									>
										{{ option.title }}
									</SelectItem>
								</SelectContent>
							</Select>
						</SettingRow>

						<SettingRow
							v-if="patternDef"
							:label="labelOf(patternDef)"
							:hint="hintOf(patternDef)"
						>
							<TitlePatternField
								v-model="values[patternDef.id]"
								:label="labelOf(patternDef)"
							/>
						</SettingRow>

						<div
							v-if="compactDefs.length"
							class="grid grid-cols-3 gap-2.5 lg:block"
						>
							<SettingRow
								v-for="def in compactDefs"
								:key="def.id"
								:label="labelOf(def)"
								:short-label="shortLabelOf(def)"
								:hint="hintOf(def)"
								width="sm"
							>
								<component :is="SettingControl" :def="def" />
							</SettingRow>
						</div>

						<SettingRow
							v-for="def in otherDefs"
							:key="def.id"
							:label="labelOf(def)"
							:hint="hintOf(def)"
						>
							<component :is="SettingControl" :def="def" />
						</SettingRow>
					</template>
				</template>
			</form>

			<template v-if="!isCreate && category">
				<div v-if="cursorEnabled || githubEnabled" class="max-lg:hidden">
					<SectionHead title="Integrations" />
					<CategoryCursorSettings
						v-if="cursorEnabled"
						:category-id="category.id"
						:is-create="false"
						class="!mt-2 !border-t-0 !pt-2"
						@cursor-configured="
							onCursor('Cursor API key configured successfully')
						"
						@cursor-removed="onCursor('Cursor API key removed')"
					/>
					<CategoryGitHubSettings
						v-if="githubEnabled"
						:category-id="category.id"
						:is-create="false"
						@connected="notify('Success', `Connected to ${$event}`)"
						@disconnected="notify('Success', 'Repository disconnected')"
						@synced="notify('Success', 'Repository synced successfully')"
						@relinked="onRelinked"
					/>
				</div>

				<SectionHead title="Danger zone" danger />
				<SettingRow
					label="Move category"
					hint="With its subcategories and tasks"
				>
					<Button
						type="button"
						variant="outline"
						class="h-8 border-line-strong bg-surface px-3 text-[13px] text-ink"
						@click="openTransfer"
					>
						Move to workspace…
					</Button>
				</SettingRow>
				<SettingRow label="Delete category">
					<Button
						type="button"
						variant="outline"
						class="h-8 border-line-strong bg-surface px-3 text-[13px] text-destructive hover:text-destructive"
						@click="deleteOpen = true"
					>
						Delete…
					</Button>
				</SettingRow>
			</template>
		</div>

		<div
			class="flex items-center justify-end gap-2 border-t border-line pb-5 pt-3 max-lg:-mx-4 max-lg:px-4 lg:px-7 lg:pb-3 max-lg:[body:has(.tour-host--sheet)_&]:mb-[50vh]"
		>
			<Button
				type="button"
				variant="ghost"
				class="h-10 text-ink-muted hover:bg-surface-hover lg:h-9"
				:class="!isCreate && 'max-lg:hidden'"
				:disabled="saving || (!isCreate && !dirty)"
				@click="isCreate ? $emit('close') : discard()"
			>
				{{ isCreate ? 'Cancel' : 'Discard' }}
			</Button>
			<Button
				type="submit"
				:form="formId"
				class="h-10 flex-1 px-[18px] text-sm font-semibold lg:h-9 lg:flex-none"
				:disabled="saving"
			>
				{{ isCreate ? 'Create' : 'Save' }}
			</Button>
		</div>

		<AlertDialog v-model:open="deleteOpen">
			<AlertDialogContent>
				<AlertDialogHeader>
					<AlertDialogTitle>Delete “{{ category?.title }}”?</AlertDialogTitle>
					<AlertDialogDescription>
						The category is removed from the list.
					</AlertDialogDescription>
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel>Cancel</AlertDialogCancel>
					<AlertDialogAction
						class="bg-destructive text-destructive-foreground shadow-sm hover:bg-destructive/90"
						@click="remove"
					>
						Delete
					</AlertDialogAction>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>

		<Dialog v-model:open="transfer.open">
			<DialogContent class="sm:max-w-[425px]">
				<DialogHeader>
					<DialogTitle>Move category to another workspace</DialogTitle>
					<DialogDescription>
						"{{ category?.title }}" will be moved with all its subcategories and
						tasks. Task statuses are matched to the target workspace by name,
						then by type; assignees who are not members of the target workspace
						are removed.
					</DialogDescription>
				</DialogHeader>
				<LegacySelect
					v-model="transfer.targetWorkspaceId"
					:options="transfer.targets"
					label-key="name"
					value-key="id"
					placeholder="Select workspace"
					class="w-full"
				/>
				<p v-if="transfer.targets.length === 0" class="text-sm text-ink-subtle">
					You are not a member of any other workspace.
				</p>
				<DialogFooter>
					<Button
						type="button"
						:disabled="!transfer.targetWorkspaceId || transfer.busy"
						@click="move"
					>
						{{ transfer.busy ? 'Moving...' : 'Move' }}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	</section>
</template>

<script>
	import {
		changeCategoryWorkspace,
		createCategory,
		deleteCategory,
		updateCategory,
	} from '@/actions/tmgr/categories';
	import { updateCategorySettings } from '@/actions/tmgr/settings';
	import { getWorkspaces } from '@/actions/tmgr/workspaces';
	import CategoryCursorSettings from '@/components/categories/CategoryCursorSettings.vue';
	import CategoryGitHubSettings from '@/components/categories/CategoryGitHubSettings.vue';
	import TitlePatternField from '@/components/categories/TitlePatternField.vue';
	import LegacySelect from '@/components/general/Select.vue';
	import TextField from '@/components/general/TextField.vue';
	import TimeField from '@/components/general/TimeField.vue';
	import {
		AlertDialog,
		AlertDialogAction,
		AlertDialogCancel,
		AlertDialogContent,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
	} from '@/components/ui/alert-dialog';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import {
		Select,
		SelectContent,
		SelectItem,
		SelectTrigger,
		SelectValue,
	} from '@/components/ui/select';
	import { Skeleton } from '@/components/ui/skeleton';
	import { Switch } from '@/components/ui/switch';
	import { useToast } from '@/components/ui/toast';
	import {
		crumbOf,
		ESTIMATE_KEY,
		formatDuration,
		hasSettings,
		parentOptions,
		pickCopySource,
		RATE_KEY,
		REMINDER_KEY,
		sanitizeCategoryCode,
		TITLE_PATTERN_KEY,
	} from '@/utils/categorySettingsPage';
	import { describeCategoryTransfer } from '@/utils/categoryTransfer';
	import generateSlugFromRu from '@/utils/generateSlugFromRu';
	import { ChevronLeft } from 'lucide-vue-next';
	import {
		computed,
		defineComponent,
		h,
		nextTick,
		onMounted,
		reactive,
		ref,
		watch,
	} from 'vue';
	import { useStore } from 'vuex';

	const LABELS = {
		[TITLE_PATTERN_KEY]: {
			label: 'Task title pattern',
			hint: 'Prefix for every new task title',
		},
		[RATE_KEY]: { label: 'Rate per hour', short: 'Rate / h' },
		[ESTIMATE_KEY]: { label: 'Default estimate', short: 'Estimate' },
		[REMINDER_KEY]: {
			label: 'Timer reminder every',
			short: 'Reminder',
			hint: 'Sound each time the running timer passes it',
		},
	};
	const COMPACT_KEYS = [RATE_KEY, ESTIMATE_KEY, REMINDER_KEY];
	const WIDTHS = {
		sm: 'lg:max-w-[160px]',
		md: 'lg:max-w-[220px]',
		lg: 'lg:max-w-[280px]',
	};
	const FIELD_CLASS =
		'h-9 rounded-lg border-line-strong bg-surface-base px-2.5 text-sm text-ink focus:border-primary focus:ring-0 focus:ring-offset-0';
	const INPUT_SKIN =
		'[&_input]:!rounded-lg [&_input]:!border-line-strong [&_input]:!bg-surface-base [&_input:focus]:!border-primary';

	const SectionHead = defineComponent({
		props: { title: String, hint: String, danger: Boolean },
		setup(props) {
			return () =>
				h('div', { class: 'flex flex-col gap-0.5 pb-1 pt-[22px]' }, [
					h(
						'span',
						{
							class: [
								'text-[11px] font-semibold uppercase tracking-[.06em]',
								props.danger ? 'text-destructive' : 'text-ink-subtle',
							],
						},
						props.title,
					),
					props.hint &&
						h(
							'span',
							{ class: 'text-xs text-ink-subtle max-lg:hidden' },
							props.hint,
						),
				]);
		},
	});

	const SettingRow = defineComponent({
		props: {
			label: String,
			shortLabel: String,
			hint: String,
			tag: String,
			width: String,
		},
		setup(props, { slots }) {
			return () =>
				h(
					'div',
					{
						class:
							'grid min-w-0 content-start gap-1.5 py-2 lg:border-b lg:border-line lg:py-3.5 xl:grid-cols-[240px_minmax(0,1fr)] xl:gap-6',
					},
					[
						h(
							'div',
							{ class: 'flex min-w-0 flex-col items-start gap-[3px] xl:pt-2' },
							[
								h('span', { class: 'text-[13px] font-semibold text-ink' }, [
									props.shortLabel
										? [
												h('span', { class: 'lg:hidden' }, props.shortLabel),
												h('span', { class: 'hidden lg:inline' }, props.label),
										  ]
										: props.label,
								]),
								props.tag &&
									h(
										'span',
										{
											class:
												'rounded bg-tag-bg px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-[.06em] text-tag-fg',
										},
										props.tag,
									),
								props.hint &&
									h(
										'span',
										{
											class:
												'hidden text-xs leading-snug text-ink-subtle lg:block',
										},
										props.hint,
									),
							],
						),
						h(
							'div',
							{ class: ['min-w-0', WIDTHS[props.width] ?? 'lg:max-w-[460px]'] },
							slots.default?.(),
						),
					],
				);
		},
	});

	export default defineComponent({
		name: 'CategorySettingsPane',
		components: {
			AlertDialog,
			AlertDialogAction,
			AlertDialogCancel,
			AlertDialogContent,
			AlertDialogDescription,
			AlertDialogFooter,
			AlertDialogHeader,
			AlertDialogTitle,
			Button,
			CategoryCursorSettings,
			CategoryGitHubSettings,
			ChevronLeft,
			Dialog,
			DialogContent,
			DialogDescription,
			DialogFooter,
			DialogHeader,
			DialogTitle,
			LegacySelect,
			SectionHead,
			Select,
			SelectContent,
			SelectItem,
			SelectTrigger,
			SelectValue,
			SettingRow,
			Skeleton,
			TextField,
			TitlePatternField,
		},
		props: {
			node: { type: Object, default: null },
			parentNode: { type: Object, default: null },
			isCreate: { type: Boolean, default: false },
			nodes: { type: Array, required: true },
			catalogue: { type: Array, default: null },
			catalogueState: { type: String, default: 'loading' },
			workspaceName: { type: String, default: '' },
			tasksLink: { type: [Object, String], default: null },
		},
		emits: [
			'dirty',
			'close',
			'add-sub',
			'saved',
			'created',
			'deleted',
			'moved',
			'integration-changed',
		],
		setup(props, { emit }) {
			const store = useStore();
			const { toast } = useToast();
			const formId = `category-settings-${Math.random().toString(36).slice(2)}`;
			const category = computed(() => props.node?.category ?? null);
			const parent = computed(() => props.parentNode?.category ?? null);
			const form = reactive({ title: '', code: '', parent: 'none' });
			const values = reactive({});
			const custom = reactive({});
			const errors = ref({});
			const saving = ref(false);
			const initial = ref(null);
			const copyFrom = ref('none');
			const nameField = ref(null);
			const deleteOpen = ref(false);
			const transfer = reactive({
				open: false,
				targets: [],
				targetWorkspaceId: null,
				busy: false,
			});

			const cursorEnabled = computed(() =>
				store.getters['featureToggles/isFeatureEnabled']('cursor'),
			);
			const githubEnabled = computed(() =>
				store.getters['featureToggles/isFeatureEnabled']('github'),
			);

			const heading = computed(() => {
				if (props.isCreate)
					return parent.value ? 'New subcategory' : 'New category';
				return category.value?.title ?? '';
			});
			const crumb = computed(() => {
				if (props.isCreate) {
					return parent.value
						? crumbOf(
								[...props.parentNode.ancestors, parent.value],
								props.workspaceName,
						  )
						: crumbOf([], props.workspaceName);
				}
				return crumbOf(props.node?.ancestors ?? [], props.workspaceName);
			});

			const parentChoices = computed(() =>
				parentOptions(props.nodes, category.value?.id ?? null),
			);
			const defs = computed(() => props.catalogue ?? []);
			const patternDef = computed(() =>
				defs.value.find((d) => d.key === TITLE_PATTERN_KEY),
			);
			const compactDefs = computed(() =>
				COMPACT_KEYS.map((key) => defs.value.find((d) => d.key === key)).filter(
					Boolean,
				),
			);
			const otherDefs = computed(() =>
				defs.value.filter(
					(d) => d.key !== TITLE_PATTERN_KEY && !COMPACT_KEYS.includes(d.key),
				),
			);
			const copyChoices = computed(() =>
				props.nodes.map((n) => n.category).filter(hasSettings),
			);
			const copyDefault = computed(() =>
				props.isCreate
					? pickCopySource(props.nodes, parent.value?.id ?? null)
					: null,
			);

			const labelOf = (def) => LABELS[def.key]?.label ?? def.name;
			const shortLabelOf = (def) => LABELS[def.key]?.short ?? def.name;
			const hintOf = (def) => LABELS[def.key]?.hint ?? '';

			const current = () => ({
				title: form.title,
				code: form.code,
				parent: form.parent,
				values: defs.value.map((d) => String(values[d.id] ?? '')),
			});
			const dirty = computed(
				() =>
					!!initial.value &&
					JSON.stringify(current()) !== JSON.stringify(initial.value),
			);
			watch(dirty, (value) => emit('dirty', value), { immediate: true });

			const fillSettings = (source) => {
				for (const def of defs.value) {
					const found = (source?.settings || []).find((s) => s.id === def.id);
					const value = found?.value ?? '';
					values[def.id] = value;
					custom[def.id] =
						!def.default_values?.length ||
						!def.default_values.some((v) => String(v.value) === String(value));
				}
			};

			const reset = () => {
				const c = category.value;
				form.title = c?.title ?? '';
				form.code = c?.code ?? '';
				form.parent = props.isCreate
					? parent.value
						? String(parent.value.id)
						: 'none'
					: c?.project_category_id != null
					? String(c.project_category_id)
					: 'none';
				errors.value = {};
				fillDefaults();
				initial.value = current();
			};

			const fillDefaults = () => {
				if (props.isCreate) {
					copyFrom.value = copyDefault.value
						? String(copyDefault.value.id)
						: 'none';
					fillSettings(copyDefault.value);
				} else {
					fillSettings(category.value);
				}
			};

			watch(
				() => props.catalogue,
				() => {
					fillDefaults();
					if (initial.value)
						initial.value = { ...initial.value, values: current().values };
				},
			);

			const applyCopy = (id) => {
				copyFrom.value = id;
				fillSettings(
					props.nodes.find((n) => String(n.category.id) === id)?.category,
				);
			};

			const discard = () => reset();

			const notify = (title, description = '', variant) =>
				toast({ title, description, ...(variant ? { variant } : {}) });

			const settingsPayload = () =>
				defs.value.map((d) => ({ id: d.id, value: values[d.id] ?? '' }));

			const fail = (e, title) => {
				const data = e?.response?.data;
				if (data?.errors && typeof data.errors === 'object') {
					errors.value = data.errors;
					if (
						['title', 'code', 'project_category_id'].some((k) => data.errors[k])
					)
						return;
				}
				notify(title, data?.message || 'Please try again.', 'destructive');
			};

			const parentId = () =>
				form.parent === 'none' ? null : Number(form.parent);

			const save = async () => {
				const c = category.value;
				const payload = {
					id: c.id,
					title: form.title.trim(),
					code: sanitizeCategoryCode(form.code, true),
					slug: c.slug,
					project_category_id: parentId(),
					workspace_id: c.workspace_id,
				};
				let saved;
				try {
					saved = await updateCategory(c.id, payload);
				} catch (e) {
					fail(e, 'Could not save the category');
					return;
				}
				const kept = [];
				form.code = payload.code;
				if (
					saved &&
					'code' in saved &&
					payload.code !== (c.code ?? '') &&
					(saved.code ?? '') !== payload.code
				) {
					form.code = saved.code ?? '';
					kept.push('code');
				}
				if (
					saved &&
					'project_category_id' in saved &&
					payload.project_category_id !== (c.project_category_id ?? null) &&
					(saved.project_category_id ?? null) !== payload.project_category_id
				) {
					form.parent =
						saved.project_category_id != null
							? String(saved.project_category_id)
							: 'none';
					kept.push('parent');
				}
				if (props.catalogueState === 'ready') {
					try {
						await updateCategorySettings(c.id, settingsPayload());
					} catch (e) {
						notify(
							'Defaults for new tasks were not saved',
							e?.response?.data?.message || 'Please try again.',
							'destructive',
						);
						emit('saved', c.id);
						return;
					}
				}
				initial.value = current();
				if (kept.length) {
					notify(
						`The server kept the old ${kept.join(' and ')}`,
						'Other changes were saved.',
						'destructive',
					);
				} else {
					notify('Saved', 'The category was saved');
				}
				emit('saved', c.id);
			};

			const create = async () => {
				const title = form.title.trim();
				const payload = {
					title,
					code: sanitizeCategoryCode(form.code, true),
					slug: generateSlugFromRu(title),
				};
				if (parentId()) payload.project_category_id = parentId();
				let created;
				try {
					created = await createCategory(payload);
				} catch (e) {
					fail(e, 'Could not create the category');
					return;
				}
				const settings = settingsPayload();
				let settingsFailed = false;
				if (
					props.catalogueState === 'ready' &&
					settings.some((s) => s.value !== '' && s.value != null)
				) {
					try {
						await updateCategorySettings(created.id, settings);
					} catch {
						settingsFailed = true;
					}
				}
				initial.value = null;
				emit('dirty', false);
				if (settingsFailed) {
					notify(
						'Category created',
						'Its defaults for new tasks were not saved. Set them again and save.',
						'destructive',
					);
				} else {
					notify('Category created');
				}
				emit('created', created.id);
			};

			const submit = async () => {
				if (saving.value) return;
				errors.value = {};
				saving.value = true;
				try {
					await (props.isCreate ? create() : save());
				} finally {
					saving.value = false;
				}
			};

			const remove = async () => {
				try {
					await deleteCategory(category.value.id);
				} catch (e) {
					notify(
						'Could not delete the category',
						e?.response?.data?.message || 'Please try again.',
						'destructive',
					);
					return;
				}
				initial.value = null;
				emit('dirty', false);
				notify('Category deleted');
				emit('deleted');
			};

			const openTransfer = async () => {
				const current = Number(store.getters.currentWorkspaceId);
				try {
					const workspaces = await getWorkspaces();
					transfer.targets = workspaces.filter((w) => Number(w.id) !== current);
				} catch (e) {
					notify(
						'Could not load workspaces',
						'Please try again.',
						'destructive',
					);
					return;
				}
				transfer.targetWorkspaceId = null;
				transfer.busy = false;
				transfer.open = true;
			};

			const move = async () => {
				const c = category.value;
				if (!c || !transfer.targetWorkspaceId) return;
				transfer.busy = true;
				try {
					const result = await changeCategoryWorkspace(
						c.id,
						Number(transfer.targetWorkspaceId),
					);
					const target = transfer.targets.find(
						(w) => Number(w.id) === Number(transfer.targetWorkspaceId),
					);
					notify(
						`"${c.title}" moved to ${target?.name ?? 'another workspace'}`,
						describeCategoryTransfer(result),
					);
					transfer.open = false;
					initial.value = null;
					emit('dirty', false);
					emit('moved');
				} catch (e) {
					notify(
						'Transfer failed',
						e?.response?.data?.message ||
							'Could not move the category. Please try again.',
						'destructive',
					);
				} finally {
					transfer.busy = false;
				}
			};

			const onCursor = (message) => {
				notify('Success', message);
				emit('integration-changed');
			};

			const onRelinked = (stats) => {
				const total = stats.commits + stats.branches + stats.pull_requests;
				notify(
					'Success',
					`Relinked ${total} items: ${stats.commits} commits, ${stats.branches} branches, ${stats.pull_requests} PRs`,
				);
			};

			const SettingControl = defineComponent({
				props: { def: { type: Object, required: true } },
				setup(p) {
					return () => {
						const def = p.def;
						const label = labelOf(def);
						const canCustom = def.custom_value_available !== false;
						const withDefaults = def.default_values?.length > 0;
						const showCustom = custom[def.id] && canCustom;
						const control = showCustom
							? def.component_type === 'time_in_seconds'
								? h(TimeField, {
										modelValue: values[def.id],
										'onUpdate:modelValue': (v) => (values[def.id] = v),
										placeholder: def.description,
										extraClass: 'h-9 font-mono text-[13px]',
										class: INPUT_SKIN,
								  })
								: h('div', { class: ['relative', INPUT_SKIN] }, [
										h(TextField, {
											modelValue: values[def.id],
											'onUpdate:modelValue': (v) => (values[def.id] = v),
											placeholder:
												def.key === RATE_KEY ? '—' : def.description || '',
											inputClass: [
												'h-9 font-mono text-[13px]',
												def.key === RATE_KEY ? 'pr-9' : '',
											].join(' '),
											showErrorInTooltip: false,
										}),
										def.key === RATE_KEY &&
											h(
												'span',
												{
													class:
														'pointer-events-none absolute right-2.5 top-2 text-[13px] text-ink-subtle',
												},
												'/ h',
											),
								  ])
							: h(
									Select,
									{
										modelValue:
											values[def.id] === '' || values[def.id] == null
												? undefined
												: String(values[def.id]),
										'onUpdate:modelValue': (v) => (values[def.id] = v),
									},
									() => [
										h(
											SelectTrigger,
											{ class: FIELD_CLASS, 'aria-label': label },
											() =>
												h(SelectValue, {
													placeholder: def.description || 'Select',
												}),
										),
										h(SelectContent, null, () =>
											(def.default_values || []).map((option) =>
												h(
													SelectItem,
													{
														key: option.id ?? option.value,
														value: String(option.value),
													},
													() =>
														def.component_type === 'time_in_seconds'
															? formatDuration(option.value) ||
															  String(option.value)
															: String(option.value),
												),
											),
										),
									],
							  );
						return h('div', { class: 'flex min-w-0 flex-col gap-2' }, [
							control,
							canCustom &&
								withDefaults &&
								h(
									'label',
									{ class: 'flex items-center gap-2 text-xs text-ink-muted' },
									[
										h(Switch, {
											checked: !!custom[def.id],
											'onUpdate:checked': (v) => (custom[def.id] = v),
										}),
										'Custom value',
									],
								),
						]);
					};
				},
			});

			onMounted(async () => {
				reset();
				if (props.isCreate) {
					await nextTick();
					nameField.value?.querySelector('input')?.focus();
				}
			});

			return {
				formId,
				category,
				form,
				values,
				errors,
				saving,
				dirty,
				copyFrom,
				nameField,
				deleteOpen,
				transfer,
				cursorEnabled,
				githubEnabled,
				heading,
				crumb,
				parentChoices,
				patternDef,
				compactDefs,
				otherDefs,
				copyChoices,
				copyDefault,
				fieldClass: FIELD_CLASS,
				inputSkin: INPUT_SKIN,
				labelOf,
				shortLabelOf,
				hintOf,
				sanitizeCategoryCode,
				applyCopy,
				discard,
				notify,
				submit,
				remove,
				openTransfer,
				move,
				onCursor,
				onRelinked,
				SettingControl,
			};
		},
	});
</script>
