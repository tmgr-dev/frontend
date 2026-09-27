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
	children: [
		{ type: 'heading', text: 'Kitchen Sink view', level: 1 },
		{
			type: 'text',
			text: props?.task ? `Opened from a deep link for task ${props.task}` : 'Opened with no task param',
			tone: 'muted',
		},
	],
}));

tmgr.events.on('app.started', async () => {
	console.info('kitchen-sink: app.started');
});

tmgr.events.on('workspace.switched', async (payload) => {
	console.info(`kitchen-sink: workspace.switched from ${payload.from} to ${payload.to}`);
});

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
