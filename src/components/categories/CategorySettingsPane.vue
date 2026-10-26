<template>
	<div class="flex min-w-0 flex-col gap-8">
		<div
			v-if="!isCreate && issues.length"
			class="flex flex-wrap items-center gap-x-3 gap-y-2 rounded-lg bg-status-progress-bg px-3 py-2.5 text-[13px] text-ink dark:bg-[oklch(0.3_0.05_75/0.35)]"
		>
			<TriangleAlert
				class="size-4 shrink-0 text-status-progress"
				aria-hidden="true"
			/>
			<span class="min-w-0 flex-1">{{ issuesText }}</span>
			<Button
				v-if="catalogueState === 'ready'"
				type="button"
				variant="outline"
				size="sm"
				class="h-7 border-line-strong bg-surface text-xs text-ink"
				:disabled="fixing || dirty"
				:title="dirty ? 'Save or discard your changes first' : undefined"
				@click="fixAll"
			>
				Fix all {{ issues.length }}
			</Button>
		</div>

		<form
			:id="formId"
			novalidate
			class="flex flex-col gap-8"
			@submit.prevent="submit"
		>
			<section :aria-labelledby="`${formId}-general`" class="flex flex-col">
				<SectionHead :id="`${formId}-general`" title="General" />
				<SettingRow label="Name" :label-for="`${formId}-name`">
					<input
						:id="`${formId}-name`"
						ref="nameField"
						v-model="form.title"
						:data-autofocus="isCreate || undefined"
						type="text"
						autocomplete="off"
						placeholder="Name"
						:aria-invalid="!!errors.title || undefined"
						:class="[inputClass, 'max-w-[360px]', errors.title && errorClass]"
					/>
					<p v-if="errors.title" class="text-sm text-status-fix-fg">
						{{ errors.title[0] }}
					</p>
				</SettingRow>
				<SettingRow
					label="Code"
					hint="Letters and digits"
					:label-for="`${formId}-code`"
				>
					<input
						:id="`${formId}-code`"
						:value="form.code"
						type="text"
						autocomplete="off"
						spellcheck="false"
						placeholder="e.g., PROJ, FEAT, BUG"
						:aria-invalid="!!errors.code || undefined"
						:class="[
							inputClass,
							'max-w-[220px] font-mono',
							errors.code && errorClass,
						]"
						@input="onCode"
					/>
					<p v-if="errors.code" class="text-sm text-status-fix-fg">
						{{ errors.code[0] }}
					</p>
				</SettingRow>
				<SettingRow label="Parent" :label-for="`${formId}-parent`">
					<Select v-model="form.parent">
						<SelectTrigger
							:id="`${formId}-parent`"
							:class="[fieldClass, 'max-w-[260px]']"
							aria-label="Parent"
						>
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
						class="text-sm text-status-fix-fg"
					>
						{{ errors.project_category_id[0] }}
					</p>
				</SettingRow>
			</section>

			<section
				v-if="catalogueState !== 'unavailable'"
				:aria-labelledby="`${formId}-new`"
				class="flex flex-col"
			>
				<SectionHead
					:id="`${formId}-new`"
					title="New tasks"
					hint="Defaults for tasks created in this category"
				/>
				<div
					v-if="catalogueState === 'loading'"
					class="space-y-3 border-t border-line py-4"
				>
					<Skeleton v-for="n in 3" :key="n" class="h-9 w-full" />
				</div>
				<template v-else>
					<SettingRow
						v-if="isCreate && copyDefault"
						label="Copy settings from"
						hint="Fills the fields below, you can still change them"
						:label-for="`${formId}-copy`"
					>
						<Select :model-value="copyFrom" @update:model-value="applyCopy">
							<SelectTrigger
								:id="`${formId}-copy`"
								:class="[fieldClass, 'max-w-[260px]']"
								aria-label="Copy settings from"
							>
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value="none">Don’t copy</SelectItem>
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

					<SettingRow
						v-for="def in [...compactDefs, ...otherDefs]"
						:key="def.id"
						:label="labelOf(def)"
						:hint="hintOf(def)"
						:label-for="controlId(def)"
					>
						<component :is="SettingControl" :def="def" />
					</SettingRow>
				</template>
			</section>
		</form>

		<p v-if="isCreate" class="text-[13px] text-ink-muted">
			Integrations and the danger zone appear after the category is created.
		</p>

		<template v-if="!isCreate && category">
			<section
				v-if="cursorEnabled || githubEnabled || agentRuns"
				:aria-labelledby="`${formId}-int`"
				class="flex flex-col max-lg:hidden"
			>
				<SectionHead :id="`${formId}-int`" title="Integrations" />
				<SettingRow
					v-if="cursorEnabled"
					label="Cursor AI agent"
					hint="Send tasks from this category to Cursor agents"
					inline
				>
					<StatusPill
						:done="!!category.cursor_connected"
						:text="category.cursor_connected ? 'Connected' : 'Not connected'"
					/>
					<Button
						type="button"
						variant="outline"
						size="sm"
						:class="[smallButtonClass, 'ml-auto']"
						:aria-expanded="open.cursor"
						@click="toggle('cursor')"
					>
						{{
							open.cursor
								? 'Close'
								: category.cursor_connected
									? 'Manage'
									: 'Connect'
						}}
					</Button>
					<template v-if="open.cursor" #below>
						<CategoryCursorSettings
							:category-id="category.id"
							:is-create="false"
							class="!mt-0 !border-t-0 !pt-0"
							@cursor-configured="
								onCursor('Cursor API key configured successfully')
							"
							@cursor-removed="onCursor('Cursor API key removed')"
						/>
					</template>
				</SettingRow>
				<SettingRow
					v-if="githubEnabled"
					label="GitHub"
					hint="Link tasks to a repository"
					inline
				>
					<StatusPill
						v-if="github !== undefined"
						:done="!!github?.repository"
						:text="
							github === null
								? 'Unavailable'
								: github.repository?.full_name || 'Not connected'
						"
					/>
					<Button
						type="button"
						variant="outline"
						size="sm"
						:class="[smallButtonClass, 'ml-auto']"
						:aria-expanded="open.github"
						@click="toggle('github')"
					>
						{{
							open.github
								? 'Close'
								: github && !github.has_installation
									? 'Install GitHub App'
									: 'Manage'
						}}
					</Button>
					<template v-if="open.github" #below>
						<CategoryGitHubSettings
							:category-id="category.id"
							:is-create="false"
							class="!mt-0 !border-t-0 !pt-0"
							@connected="onGitHub(`Connected to ${$event}`)"
							@disconnected="onGitHub('Repository disconnected')"
							@synced="notify('Success', 'Repository synced successfully')"
							@relinked="onRelinked"
						/>
					</template>
				</SettingRow>
				<SettingRow
					v-if="agentRuns"
					label="Agent repositories"
					hint="Where agent runs may work, as owner/name"
					inline
				>
					<StatusPill v-if="repos" :text="reposText" />
					<Button
						type="button"
						variant="outline"
						size="sm"
						:class="[smallButtonClass, 'ml-auto']"
						:aria-expanded="open.repos"
						@click="toggle('repos')"
					>
						{{
							open.repos ? 'Close' : repos?.length ? 'Edit' : 'Add repository'
						}}
					</Button>
					<template v-if="open.repos" #below>
						<CategoryAgentRepositories
							:category-id="category.id"
							class="!mt-0 !border-t-0 !pt-0"
						/>
					</template>
				</SettingRow>
			</section>

			<section :aria-labelledby="`${formId}-danger`" class="flex flex-col">
				<SectionHead :id="`${formId}-danger`" title="Danger zone" danger />
				<SettingRow
					label="Move to another workspace"
					hint="With its subcategories and tasks"
					inline
				>
					<Button
						type="button"
						variant="outline"
						size="sm"
						:class="[smallButtonClass, 'ml-auto']"
						:disabled="done"
						@click="openTransfer"
					>
						<ArrowRightLeft aria-hidden="true" />
						Move…
					</Button>
				</SettingRow>
				<SettingRow
					label="Delete category"
					hint="The category is removed from the list"
					inline
				>
					<Button
						type="button"
						variant="outline"
						size="sm"
						:class="[
							smallButtonClass,
							'ml-auto !text-status-fix-fg hover:!text-status-fix-fg',
						]"
						:disabled="done"
						@click="deleteOpen = true"
					>
						<Trash2 aria-hidden="true" />
						Delete…
					</Button>
				</SettingRow>
			</section>
		</template>

		<div
			v-if="isCreate || dirty"
			class="sticky bottom-[calc(var(--statusbar-h,0px)+1rem)] z-10 flex flex-wrap items-center gap-x-3 gap-y-2.5 rounded-xl border border-line-strong bg-surface py-2.5 pr-2.5 pl-4 shadow-tmgr-lg max-lg:[body:has(.tour-host--sheet)_&]:mb-[50vh]"
		>
			<span class="text-[13px] font-medium text-ink">{{ barText }}</span>
			<Button
				type="button"
				variant="ghost"
				size="sm"
				class="ml-auto h-8 px-3 text-[13px] text-ink-muted hover:bg-surface-hover hover:text-ink"
				:disabled="saving || done"
				@click="isCreate ? $emit('close') : discard()"
			>
				{{ isCreate ? 'Cancel' : 'Discard' }}
			</Button>
			<Button
				type="submit"
				:form="formId"
				size="sm"
				class="h-8 px-4 text-[13px] font-semibold"
				:disabled="saving || done || invalid"
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
						class="bg-destructive text-destructive-foreground shadow-xs hover:bg-destructive/90"
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
	</div>
</template>

<script>
	import {
		getAgentRunsSetting,
		getCategoryAgentRepositories,
	} from '@/actions/tmgr/agentRuns';
	import {
		changeCategoryWorkspace,
		createCategory,
		deleteCategory,
		updateCategory,
	} from '@/actions/tmgr/categories';
	import { getCategoryGitHubStatus } from '@/actions/tmgr/github';
	import { updateCategorySettings } from '@/actions/tmgr/settings';
	import { getWorkspaces } from '@/actions/tmgr/workspaces';
	import CategoryAgentRepositories from '@/components/categories/CategoryAgentRepositories.vue';
	import CategoryCursorSettings from '@/components/categories/CategoryCursorSettings.vue';
	import CategoryGitHubSettings from '@/components/categories/CategoryGitHubSettings.vue';
	import TitlePatternField from '@/components/categories/TitlePatternField.vue';
	import LegacySelect from '@/components/general/Select.vue';
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
		ESTIMATE_KEY,
		fixedSettingsPayload,
		formatDuration,
		hasSettings,
		literalIssues,
		parentOptions,
		parseDuration,
		pickCopySource,
		RATE_KEY,
		REMINDER_KEY,
		sanitizeCategoryCode,
		TITLE_PATTERN_KEY,
	} from '@/utils/categorySettingsPage';
	import { describeCategoryTransfer } from '@/utils/categoryTransfer';
	import generateSlugFromRu from '@/utils/generateSlugFromRu';
	import { ArrowRightLeft, Trash2, TriangleAlert } from 'lucide-vue-next';
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
		[RATE_KEY]: { label: 'Rate per hour' },
		[ESTIMATE_KEY]: { label: 'Default estimate' },
		[REMINDER_KEY]: {
			label: 'Timer reminder every',
			hint: 'Sound each time the running timer passes it',
		},
	};
	const COMPACT_KEYS = [RATE_KEY, ESTIMATE_KEY, REMINDER_KEY];
	const DURATION_ERROR = 'Use a duration like 45m, 2h or 1h 30m';
	const FIELD_CLASS =
		'h-9 rounded-lg border-line-strong bg-surface-base px-2.5 text-sm text-ink';
	const INPUT_CLASS =
		'h-9 w-full rounded-lg border border-line-strong bg-surface-base px-2.5 text-sm text-ink outline-hidden transition-colors placeholder:text-ink-subtle focus:border-line-focus';
	const ERROR_CLASS = '!border-status-fix';
	const SMALL_BUTTON_CLASS =
		'h-8 border-line-strong bg-surface px-3 text-[13px] text-ink hover:bg-surface-hover hover:text-ink';

	const SectionHead = defineComponent({
		props: { id: String, title: String, hint: String, danger: Boolean },
		setup(props) {
			return () => [
				h(
					'h2',
					{
						id: props.id,
						class: [
							'text-[11px] font-semibold uppercase leading-tight tracking-[.04em]',
							props.danger ? 'text-status-fix-fg' : 'text-ink-muted',
							!props.hint && 'pb-2.5',
						],
					},
					props.title,
				),
				props.hint &&
					h(
						'p',
						{ class: 'mb-2.5 mt-1 text-[13px] text-ink-muted' },
						props.hint,
					),
			];
		},
	});

	const SettingRow = defineComponent({
		props: {
			label: String,
			hint: String,
			labelFor: String,
			inline: Boolean,
		},
		setup(props, { slots }) {
			return () =>
				h(
					'div',
					{
						class: [
							'flex flex-wrap gap-x-8 gap-y-2 border-t border-line py-4',
							props.inline && 'items-center',
						],
					},
					[
						h('div', { class: 'min-w-0 max-w-[240px] flex-[1_1_200px]' }, [
							h(
								props.labelFor ? 'label' : 'span',
								{
									for: props.labelFor,
									class: 'block text-sm font-medium text-ink',
								},
								props.label,
							),
							props.hint &&
								h(
									'span',
									{
										class: 'mt-0.5 block text-xs leading-normal text-ink-muted',
									},
									props.hint,
								),
						]),
						h(
							'div',
							{
								class: [
									'flex min-w-0 flex-[999_1_320px]',
									props.inline
										? 'flex-wrap items-center gap-3'
										: 'flex-col gap-2',
								],
							},
							slots.default?.(),
						),
						slots.below &&
							h('div', { class: 'w-full min-w-0 pt-2' }, slots.below()),
					],
				);
		},
	});

	const StatusPill = defineComponent({
		props: { text: String, done: Boolean },
		setup(props) {
			return () =>
				h(
					'span',
					{
						class: [
							'inline-flex h-[22px] max-w-full items-center gap-1.5 truncate rounded-full px-2.5 text-xs font-medium',
							props.done
								? 'bg-status-done-bg text-status-done-fg'
								: 'bg-tag-bg text-tag-fg',
						],
					},
					[
						props.done &&
							h('span', {
								'aria-hidden': 'true',
								class: 'size-1.5 shrink-0 rounded-full bg-current',
							}),
						props.text,
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
			ArrowRightLeft,
			Button,
			CategoryCursorSettings,
			CategoryAgentRepositories,
			CategoryGitHubSettings,
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
			StatusPill,
			TitlePatternField,
			Trash2,
			TriangleAlert,
		},
		props: {
			node: { type: Object, default: null },
			parentNode: { type: Object, default: null },
			isCreate: { type: Boolean, default: false },
			nodes: { type: Array, required: true },
			catalogue: { type: Array, default: null },
			catalogueState: { type: String, default: 'loading' },
			workspaceName: { type: String, default: '' },
		},
		emits: [
			'dirty',
			'close',
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
			const durationText = reactive({});
			const focusedDuration = ref(null);
			const errors = ref({});
			const saving = ref(false);
			const done = ref(false);
			const fixing = ref(false);
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
			const open = reactive({ cursor: false, github: false, repos: false });
			const github = ref(undefined);
			const agentRuns = ref(false);
			const repos = ref(null);

			const cursorEnabled = computed(() =>
				store.getters['featureToggles/isFeatureEnabled']('cursor'),
			);
			const githubEnabled = computed(() =>
				store.getters['featureToggles/isFeatureEnabled']('github'),
			);

			const issues = computed(() => literalIssues(props.nodes));
			const issuesText = computed(() =>
				issues.value.length === 1
					? '1 category names new tasks literally'
					: `${issues.value.length} categories name new tasks literally`,
			);
			const reposText = computed(() => {
				const count = repos.value?.length ?? 0;
				if (!count) return 'None';
				return count === 1 ? '1 repository' : `${count} repositories`;
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
			const hintOf = (def) => LABELS[def.key]?.hint ?? '';
			const controlId = (def) => `${formId}-setting-${def.id}`;
			const isDuration = (def) => def.component_type === 'time_in_seconds';
			const showsCustom = (def) =>
				!!custom[def.id] && def.custom_value_available !== false;

			const invalidIds = computed(() =>
				defs.value
					.filter(
						(d) =>
							isDuration(d) &&
							showsCustom(d) &&
							(durationText[d.id] ?? '').trim() !== '' &&
							parseDuration(durationText[d.id]) === null,
					)
					.map((d) => d.id),
			);
			const invalid = computed(() => invalidIds.value.length > 0);

			const current = () => ({
				title: form.title,
				code: form.code,
				parent: form.parent,
				values: defs.value.map((d) => String(values[d.id] ?? '')),
			});
			const changes = computed(() => {
				if (!initial.value) return 0;
				const now = current();
				const fields = ['title', 'code', 'parent'].filter(
					(key) => now[key] !== initial.value[key],
				).length;
				const settings = defs.value.filter(
					(d, i) =>
						now.values[i] !== initial.value.values[i] ||
						invalidIds.value.includes(d.id),
				).length;
				return fields + settings;
			});
			const dirty = computed(() => changes.value > 0);
			watch(dirty, (value) => emit('dirty', value), { immediate: true });

			const barText = computed(() => {
				if (props.isCreate)
					return parent.value
						? `New subcategory of ${parent.value.title}`
						: 'New top-level category';
				return `${changes.value} unsaved ${
					changes.value === 1 ? 'change' : 'changes'
				}`;
			});

			const fillSettings = (source) => {
				for (const def of defs.value) {
					const found = (source?.settings || []).find((s) => s.id === def.id);
					const value = found?.value ?? '';
					values[def.id] = value;
					if (isDuration(def)) durationText[def.id] = formatDuration(value);
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

			watch(
				[
					() => props.isCreate,
					() => props.node,
					() => props.parentNode?.category?.id ?? null,
				],
				([isCreate, node, parentId], [wasCreate, oldNode, oldParentId]) => {
					const same =
						isCreate === wasCreate &&
						parentId === oldParentId &&
						node?.category?.id === oldNode?.category?.id;
					if (!same || (!isCreate && !dirty.value)) reset();
				},
			);

			const applyCopy = (id) => {
				copyFrom.value = id;
				fillSettings(
					props.nodes.find((n) => String(n.category.id) === id)?.category,
				);
			};

			const onCode = (event) => {
				form.code = sanitizeCategoryCode(event.target.value);
				if (event.target.value !== form.code) event.target.value = form.code;
			};

			const onDurationInput = (def, text) => {
				durationText[def.id] = text;
				if (!text.trim()) {
					values[def.id] = '';
					return;
				}
				const seconds = parseDuration(text);
				if (seconds !== null) values[def.id] = String(seconds);
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
				done.value = true;
				emit('created', created.id);
			};

			const submit = async () => {
				if (saving.value || done.value || invalid.value) return;
				if (!props.isCreate && !dirty.value) return;
				errors.value = {};
				saving.value = true;
				try {
					await (props.isCreate ? create() : save());
				} finally {
					saving.value = false;
				}
			};

			const fixAll = async () => {
				const targets = issues.value.map((i) => i.category);
				fixing.value = true;
				let fixed = 0;
				for (const target of targets) {
					try {
						await updateCategorySettings(
							target.id,
							fixedSettingsPayload(target),
						);
						fixed += 1;
					} catch (e) {
						console.error(e);
					}
				}
				fixing.value = false;
				emit('saved', category.value?.id);
				if (fixed === targets.length) {
					notify(
						`Fixed ${fixed} title ${fixed === 1 ? 'pattern' : 'patterns'}`,
					);
				} else {
					notify(
						`Fixed ${fixed} of ${targets.length} title patterns`,
						`${
							targets.length - fixed
						} could not be saved. Try again for the rest.`,
						'destructive',
					);
				}
			};

			const remove = async () => {
				if (done.value) return;
				done.value = true;
				try {
					await deleteCategory(category.value.id);
				} catch (e) {
					notify(
						'Could not delete the category',
						e?.response?.data?.message || 'Please try again.',
						'destructive',
					);
					done.value = false;
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
					done.value = true;
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

			const loadGitHub = async (id) => {
				if (!githubEnabled.value) return;
				try {
					const status = await getCategoryGitHubStatus(id);
					if (category.value?.id === id) github.value = status;
				} catch {
					if (category.value?.id === id) github.value = null;
				}
			};

			const loadRepos = async (id) => {
				try {
					const enabled = await getAgentRunsSetting(
						store.getters.currentWorkspaceId,
					);
					const list = enabled ? await getCategoryAgentRepositories(id) : null;
					if (category.value?.id !== id) return;
					agentRuns.value = enabled;
					repos.value = list;
				} catch {
					if (category.value?.id === id) agentRuns.value = false;
				}
			};

			watch(
				() => (props.isCreate ? null : (category.value?.id ?? null)),
				(id) => {
					open.cursor = open.github = open.repos = false;
					github.value = undefined;
					repos.value = null;
					if (id == null) return;
					loadGitHub(id);
					loadRepos(id);
				},
				{ immediate: true },
			);

			const toggle = (key) => {
				open[key] = !open[key];
				if (open[key] || !category.value) return;
				if (key === 'github') loadGitHub(category.value.id);
				if (key === 'repos') loadRepos(category.value.id);
			};

			const onCursor = (message) => {
				notify('Success', message);
				emit('integration-changed');
			};

			const onGitHub = (message) => {
				notify('Success', message);
				if (category.value) loadGitHub(category.value.id);
			};

			const onRelinked = (stats) => {
				const total = stats.commits + stats.branches + stats.pull_requests;
				notify(
					'Success',
					`Relinked ${total} items: ${stats.commits} commits, ${stats.branches} branches, ${stats.pull_requests} PRs`,
				);
			};

			const DurationInput = (def, id) => {
				const showError =
					invalidIds.value.includes(def.id) && focusedDuration.value !== def.id;
				const hint = def.key === ESTIMATE_KEY ? 'Like 45m, 2h or 1h 30m' : '';
				const noteId = `${id}-note`;
				return h('div', { class: 'flex min-w-0 flex-col gap-1.5' }, [
					h('input', {
						id,
						type: 'text',
						autocomplete: 'off',
						spellcheck: 'false',
						placeholder: 'e.g., 45m',
						value: durationText[def.id] ?? '',
						'aria-invalid': showError || undefined,
						'aria-describedby': showError || hint ? noteId : undefined,
						class: [
							INPUT_CLASS,
							'max-w-[180px] font-mono text-[13px]',
							showError && ERROR_CLASS,
						],
						onInput: (e) => onDurationInput(def, e.target.value),
						onFocus: () => (focusedDuration.value = def.id),
						onBlur: () => (focusedDuration.value = null),
					}),
					showError
						? h(
								'p',
								{ id: noteId, class: 'text-xs text-status-fix-fg' },
								DURATION_ERROR,
							)
						: hint &&
							h('p', { id: noteId, class: 'text-xs text-ink-muted' }, hint),
				]);
			};

			const SettingControl = defineComponent({
				props: { def: { type: Object, required: true } },
				setup(p) {
					return () => {
						const def = p.def;
						const id = controlId(def);
						const label = labelOf(def);
						const canCustom = def.custom_value_available !== false;
						const withDefaults = def.default_values?.length > 0;
						const control = showsCustom(def)
							? isDuration(def)
								? DurationInput(def, id)
								: h(
										'div',
										{
											class: [
												'relative',
												def.key === RATE_KEY
													? 'max-w-[180px]'
													: 'max-w-[360px]',
											],
										},
										[
											h('input', {
												id,
												type: 'text',
												autocomplete: 'off',
												value: values[def.id] ?? '',
												placeholder:
													def.key === RATE_KEY ? '—' : def.description || '',
												class: [
													INPUT_CLASS,
													'font-mono text-[13px]',
													def.key === RATE_KEY ? 'pr-10' : '',
												],
												onInput: (e) => (values[def.id] = e.target.value),
											}),
											def.key === RATE_KEY &&
												h(
													'span',
													{
														class:
															'pointer-events-none absolute right-3 top-2 text-[13px] text-ink-muted',
													},
													'/ h',
												),
										],
									)
							: h(
									Select,
									{
										modelValue:
											values[def.id] === '' || values[def.id] == null
												? undefined
												: String(values[def.id]),
										'onUpdate:modelValue': (v) => {
											values[def.id] = v;
											if (isDuration(def))
												durationText[def.id] = formatDuration(v);
										},
									},
									() => [
										h(
											SelectTrigger,
											{
												id,
												class: [FIELD_CLASS, 'max-w-[260px]'],
												'aria-label': label,
											},
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
														isDuration(def)
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
					nameField.value?.focus();
				}
			});

			return {
				formId,
				category,
				form,
				values,
				errors,
				saving,
				done,
				fixing,
				dirty,
				invalid,
				barText,
				copyFrom,
				nameField,
				deleteOpen,
				transfer,
				open,
				github,
				agentRuns,
				repos,
				reposText,
				cursorEnabled,
				githubEnabled,
				issues,
				issuesText,
				parentChoices,
				patternDef,
				compactDefs,
				otherDefs,
				copyChoices,
				copyDefault,
				fieldClass: FIELD_CLASS,
				inputClass: INPUT_CLASS,
				errorClass: ERROR_CLASS,
				smallButtonClass: SMALL_BUTTON_CLASS,
				labelOf,
				hintOf,
				controlId,
				applyCopy,
				onCode,
				discard,
				notify,
				submit,
				fixAll,
				remove,
				openTransfer,
				move,
				toggle,
				onCursor,
				onGitHub,
				onRelinked,
				SettingControl,
			};
		},
	});
</script>
