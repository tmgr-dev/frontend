const CATEGORY_CODE = 'KS';
const STATUS_NAME = 'Needs answer';
const ALARM_NAME = 'ks-tick';
const BADGE_ID = 'ks-badges';

/** Idempotent: safe to run more than once, since the owner will click "Set up workspace" again. */
const ensureWorkspace = async () => {
	const statuses = await tmgr.statuses.list();
	let status = statuses.find((s) => s.name === STATUS_NAME);
	if (!status) status = await tmgr.statuses.create({ name: STATUS_NAME, type: 'active' });

	const categories = await tmgr.categories.list();
	let category = categories.find((c) => c.code === CATEGORY_CODE);
	if (!category) category = await tmgr.categories.create({ title: 'Kitchen Sink', code: CATEGORY_CODE });

	const setup = { statusId: status.id, categoryId: category.id };
	await tmgr.storage.set('setup', setup);
	await tmgr.ui.notify('Status and category are ready.', { title: 'Kitchen Sink: workspace set up' });
	return setup;
};

tmgr.commands.register('tmgr-dev.kitchen-sink.setup', ensureWorkspace);

const refreshTray = async () => {
	const { items } = await tmgr.tasks.list({
		categoryId: (await tmgr.storage.get('setup'))?.categoryId ?? null,
		statusType: 'active',
		sort: 'due',
		direction: 'asc',
		perPage: 5,
	});
	await tmgr.ui.setStatusBarItem('count', {
		text: `${items.length} KS tasks`,
		command: 'tmgr-dev.kitchen-sink.createSample',
	});
	await tmgr.ui.setTrayItem('ks-tray', {
		title: 'Kitchen Sink',
		items: items.map((task) => ({ title: task.title, taskId: task.id, command: null, args: null })),
	});
	try {
		await tmgr.ui.setTrayTitle(`KS ${items.length}`);
	} catch (error) {
		// Only the plugin picked in Settings -> "Menu bar text" may set it; not an error worth surfacing.
		console.warn(`tray title not set: ${error.message}`);
	}
	await tmgr.ui.setViewBadge('view', { count: items.length, tone: items.length > 3 ? 'warning' : 'info' });
	await tmgr.ui.refresh('badges', BADGE_ID);
};

const createSample = async () => {
	const setup = (await tmgr.storage.get('setup')) ?? (await ensureWorkspace());
	const task = await tmgr.tasks.create({
		title: 'Kitchen sink sample task',
		project_category_id: setup.categoryId,
		status_id: setup.statusId,
		priority: 'high',
		expired_at: new Date(Date.now() + 24 * 60 * 60 * 1000).toISOString(),
	});
	const back = await tmgr.tasks.get(task.id);
	console.info(`sample task key: ${back.key}`);

	const comment = await tmgr.comments.add(task.id, 'Created by the Kitchen Sink plugin.');
	await tmgr.comments.react(comment.id, '👍');

	const { items: others } = await tmgr.tasks.list({ perPage: 2 });
	let other = others.find((t) => t.id !== task.id);
	if (!other) {
		// The very first run has nothing else to relate to yet; make a small reference task rather than
		// silently skipping the demo.
		other = await tmgr.tasks.create({
			title: 'Kitchen sink reference task',
			project_category_id: setup.categoryId,
			status_id: setup.statusId,
		});
	}
	await tmgr.tasks.relate(task.id, other.id, 'relates to');

	await tmgr.taskData.set(task.id, 'kitchenSink.note', {
		createdBy: 'kitchen-sink',
		at: new Date().toISOString(),
	});

	const run = await tmgr.agentWork.start(task.id, {
		agent: 'kitchen-sink-bot',
		model: 'demo-1',
		sessionId: 'demo-session',
		branch: 'demo',
	});
	await tmgr.agentWork.finish(run.id, {
		status: 'succeeded',
		summary: 'Demo agent work finished.',
		commits: [],
		tests: { passed: 1, failed: 0, command: 'npm test' },
	});
	console.info(`agent work runs for this task: ${(await tmgr.agentWork.list(task.id)).length}`);

	await refreshTray();
	return back;
};

tmgr.commands.register('tmgr-dev.kitchen-sink.createSample', createSample);

tmgr.commands.register('tmgr-dev.kitchen-sink.sayHi', async (args) => {
	await tmgr.ui.notify('Hi from the section button!', { taskId: args?.taskId ?? null });
});

tmgr.commands.register('tmgr-dev.kitchen-sink.ping', async () => {
	await tmgr.ui.notify('Pinged via a tmgr:// deep link.', { title: 'Kitchen Sink' });
});

/** Local workspaces only: converts the first undone one-off note of the day into a task. */
const captureNotes = async () => {
	const setup = (await tmgr.storage.get('setup')) ?? (await ensureWorkspace());
	const now = new Date();
	const today = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
	const entries = await tmgr.routines.list({ from: today, to: today });
	const todo = entries.find((entry) => !entry.completed && !entry.recurring);
	if (!todo) {
		await tmgr.ui.notify('Nothing to capture today.', { title: 'Kitchen Sink' });
		return null;
	}
	const task = await tmgr.routines.convertToTask(todo.routineId, { categoryId: setup.categoryId });
	await tmgr.ui.notify(`Converted "${task.title}" into a task.`, { title: 'Kitchen Sink', taskId: task.id });
	return task;
};

tmgr.commands.register('tmgr-dev.kitchen-sink.captureNotes', captureNotes);

const PAGE_TITLE = 'Kitchen Sink notes';
const PAGE_BODY = [
	'# Kitchen Sink notes',
	'',
	'## Log',
	'',
].join('\n');

/** API 1.5: finds or creates a page, appends to its Log, rewrites its own managed section, keeps per-page data. */
const writePage = async () => {
	const hits = await tmgr.pages.search(PAGE_TITLE, { limit: 5 });
	const hit = hits.find((h) => h.title === PAGE_TITLE);
	let page = hit
		? await tmgr.pages.get(hit.id)
		: await tmgr.pages.create({ title: PAGE_TITLE, body: PAGE_BODY, properties: {} });
	page = await tmgr.pages.append(page.id, { markdown: `- written at ${new Date().toISOString()}`, heading: 'Log', createHeading: true });
	const runs = ((await tmgr.pageData.get(page.id, 'kitchenSink.runs')) ?? 0) + 1;
	await tmgr.pageData.set(page.id, 'kitchenSink.runs', runs);
	page = await tmgr.pages.setSection(page.id, 'kitchen-sink', `Written ${runs} time(s) by Kitchen Sink.`, { heading: 'Kitchen Sink' });
	try {
		await tmgr.pages.update(page.id, { version: page.version - 1, title: PAGE_TITLE });
	} catch (error) {
		if (error.name !== 'page_conflict') throw error;
		console.info(`kitchen-sink: page is at version ${error.current.version}`);
	}
	await tmgr.ui.notify(`Page "${page.title}" updated (version ${page.version}).`, { title: 'Kitchen Sink' });
	return page;
};

tmgr.commands.register('tmgr-dev.kitchen-sink.writePage', writePage);

tmgr.events
	.on('page.updated', async (payload) => {
		console.info(`kitchen-sink: page.updated ${payload.pageId} sections=${payload.changedSections.join(',')}`);
	})
	.catch((error) => console.warn(`page.updated listener not registered: ${error.message}`));

tmgr.commands.register('tmgr-dev.kitchen-sink.cardClicked', async (args) => {
	const via = args?.via ?? 'card';
	console.info(`kitchen-sink: cardClicked via ${via} column=${args?.column} row=${args?.row ?? ''}`);
	await tmgr.ui.notify(`Card clicked (${via})`, { title: 'Kitchen Sink' });
});

/** API 1.6: task "…" menu items; the app passes `{ taskId, workspaceId }`. */
tmgr.commands.register('tmgr-dev.kitchen-sink.commentOnTask', async (args) => {
	await tmgr.comments.add(args.taskId, 'Commented from the task menu by the Kitchen Sink plugin.');
});

tmgr.commands.register('tmgr-dev.kitchen-sink.notifyTask', async (args) => {
	const task = await tmgr.tasks.get(args.taskId);
	await tmgr.ui.notify(`Task #${task.id}: ${task.title}`, { title: 'Kitchen Sink', taskId: task.id });
});

/** API 1.3 demo: 4 lanes, each an accented card with a nested-card list and a menu. */
const KITCHEN_SINK_LANE_COLORS = ['blue', 'yellow', 'purple', 'green'];

const kitchenSinkLane = (column) => ({
	type: 'card',
	tone: 'default',
	padding: 'md',
	accent: KITCHEN_SINK_LANE_COLORS[column],
	children: [
		{
			type: 'stack',
			direction: 'row',
			justify: 'between',
			align: 'center',
			children: [
				{ type: 'stack', direction: 'row', grow: true, children: [{ type: 'heading', text: `Lane ${column + 1}`, level: 3 }] },
				{ type: 'badge', text: `${column + 1}`, color: KITCHEN_SINK_LANE_COLORS[column] },
				{
					type: 'menu',
					label: 'More',
					icon: 'more',
					items: [
						{ text: 'Say hi', command: 'tmgr-dev.kitchen-sink.sayHi', args: { taskId: null } },
						{ text: 'Ping', command: 'tmgr-dev.kitchen-sink.ping' },
						{
							text: 'Card clicked',
							command: 'tmgr-dev.kitchen-sink.cardClicked',
							args: { column, via: 'menu' },
							confirm: `Run cardClicked for lane ${column + 1}?`,
						},
					],
				},
			],
		},
		...[0, 1, 2].map((row) => ({
			type: 'card',
			tone: row === 0 ? 'raised' : 'muted',
			padding: 'sm',
			onClick: { command: 'tmgr-dev.kitchen-sink.cardClicked', args: { column, row, via: 'card' } },
			children: [
				{
					type: 'stack',
					direction: 'row',
					align: 'center',
					gap: 'sm',
					children: [
						{ type: 'text', text: `KS-${column * 3 + row + 1}`, tone: 'muted' },
						{ type: 'stack', direction: 'row', grow: true, children: [{ type: 'text', text: `Task ${row + 1}`, tone: 'default' }] },
						{ type: 'badge', text: 'HIGH', color: 'red' },
						{
							type: 'button',
							text: 'Open',
							command: 'tmgr-dev.kitchen-sink.cardClicked',
							args: { column, row, via: 'button' },
							variant: 'primary',
							size: 'sm',
						},
					],
				},
			],
		})),
	],
});

const kitchenSinkGridDemo = () => ({
	type: 'stack',
	direction: 'column',
	gap: 'md',
	children: [
		{ type: 'heading', text: 'Card & grid demo (API 1.3)', level: 2 },
		{
			type: 'grid',
			columns: 4,
			minWidth: 180,
			gap: 'md',
			children: [0, 1, 2, 3].map(kitchenSinkLane),
		},
		{
			type: 'stack',
			direction: 'row',
			gap: 'sm',
			children: [0, 1, 2, 3].map((column) => ({
				type: 'stat',
				label: `Lane ${column + 1}`,
				value: String(column * 3 + 3),
				tone: 'default',
				command: 'tmgr-dev.kitchen-sink.cardClicked',
				args: { column, via: 'stat' },
			})),
		},
	],
});

tmgr.ui.provideBadges(BADGE_ID, async (tasks) => {
	const notes = await tmgr.taskData.getMany(tasks.map((t) => t.id), 'kitchenSink.note');
	const result = {};
	for (const task of tasks) {
		const badges = [];
		if (task.priority) {
			badges.push({
				text: task.priority.toUpperCase(),
				color: task.priority === 'urgent' || task.priority === 'high' ? 'red' : 'gray',
				tooltip: `Priority: ${task.priority}`,
				priority: 2,
			});
		}
		if (notes[task.id]) {
			badges.push({ text: 'KS', color: 'blue', tooltip: 'Touched by the Kitchen Sink plugin', priority: 1, key: 'kitchen-sink' });
		}
		if (badges.length) result[task.id] = badges;
	}
	return result;
});

tmgr.ui.provideTaskSection('ks-section', async (task) => ({
	type: 'stack',
	direction: 'column',
	children: [
		{ type: 'heading', text: 'Kitchen Sink', level: 3 },
		{ type: 'copyable', text: 'resume --generate --for="Kitchen Sink Task"', label: 'Copy the fake resume command' },
		{
			type: 'keyValue',
			items: [
				{ key: 'Key', value: { type: 'text', text: task.key ?? '—', tone: 'muted' } },
				{ key: 'Priority', value: { type: 'text', text: task.priority ?? 'none', tone: 'muted' } },
			],
		},
		{ type: 'timeAgo', at: task.updated_at || new Date().toISOString() },
		...(task.expired_at ? [{ type: 'dueTime', at: task.expired_at }] : []),
		{ type: 'link', url: 'https://example.com/kitchen-sink', text: 'Kitchen Sink docs (example.com)' },
		{
			type: 'button',
			text: 'Say hi',
			command: 'tmgr-dev.kitchen-sink.sayHi',
			args: { taskId: task.id },
			confirm: 'Say hi to this task?',
		},
	],
}));

tmgr.ui.providePage('view', async (props) => ({
	type: 'stack',
	direction: 'column',
	gap: 'md',
	children: [
		{ type: 'heading', text: 'Kitchen Sink view', level: 1 },
		{
			type: 'text',
			text: props?.task ? `Opened from a deep link for task ${props.task}` : 'Opened with no task param',
			tone: 'muted',
		},
		kitchenSinkGridDemo(),
	],
}));

tmgr.events.on('app.started', async () => {
	console.info('kitchen-sink: app.started');
});

tmgr.events.on('workspace.switched', async (payload) => {
	console.info(`kitchen-sink: workspace.switched from ${payload.from} to ${payload.to}`);
});

tmgr.events
	.on('routine.created', async (payload) => {
		console.info(`kitchen-sink: routine.created ${payload.routineId}`);
	})
	.catch((error) => console.warn(`routine.created listener not registered: ${error.message}`));

tmgr.events.on('alarm', async (payload) => {
	if (payload.name !== ALARM_NAME) return;
	await refreshTray();
});

const startAlarm = async () => {
	const existing = await tmgr.alarms.list();
	if (!existing.some((a) => a.name === ALARM_NAME)) {
		await tmgr.alarms.create(ALARM_NAME, { periodMinutes: 5 });
	}
};

refreshTray().catch((error) => console.warn(`tray refresh failed: ${error.message}`));
startAlarm().catch((error) => console.warn(`alarm setup failed: ${error.message}`));
