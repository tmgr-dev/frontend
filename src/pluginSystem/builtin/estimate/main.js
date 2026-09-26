const nowSeconds = () => Math.floor(Date.now() / 1000);

const spentOf = (task) =>
	(Number(task.common_time) || 0) +
	(Number(task.start_time) > 0
		? Math.max(0, nowSeconds() - Number(task.start_time))
		: 0);

const duration = (seconds) => {
	const total = Math.max(0, Math.round(seconds / 60));
	const h = Math.floor(total / 60);
	const m = total % 60;
	return h ? `${h}h ${m}m` : `${m}m`;
};

const measure = (task, warnAt) => {
	const estimate = Number(task.approximately_time) || 0;
	if (estimate <= 0) return null;
	const spent = spentOf(task);
	const ratio = spent / estimate;
	return {
		spent,
		estimate,
		ratio,
		over: Math.max(0, spent - estimate),
		color: ratio >= 1 ? 'red' : ratio >= warnAt ? 'yellow' : 'green',
	};
};

const warnAt = async () => {
	const { warnAt: value } = await tmgr.settings.get();
	return typeof value === 'number' && value > 0 && value < 1 ? value : 0.8;
};

const allTasks = async () => {
	const tasks = [];
	for (let page = 1; page <= 10; page++) {
		const { items } = await tmgr.tasks.list({ page, perPage: 100 });
		tasks.push(...items);
		if (items.length < 100) break;
	}
	return tasks;
};

const activeStatusIds = async () => {
	const statuses = await tmgr.statuses.list();
	return new Set(statuses.filter((s) => s.type === 'active').map((s) => s.id));
};

const taskKey = (task) =>
	task.category && task.category.code && task.category_tasks_sequence_id
		? `${task.category.code}-${task.category_tasks_sequence_id}`
		: `#${task.id}`;

const overrunTasks = async () => {
	const [tasks, threshold] = await Promise.all([allTasks(), warnAt()]);
	return tasks
		.map((task) => ({ task, m: measure(task, threshold) }))
		.filter(({ m }) => m !== null)
		.sort((a, b) => b.m.ratio - a.m.ratio);
};

const updateStatusBar = async () => {
	const [measured, active] = await Promise.all([
		overrunTasks(),
		activeStatusIds(),
	]);
	const over = measured.filter(
		({ task, m }) => active.has(task.status_id) && m.over > 0,
	);
	const total = over.reduce((sum, { m }) => sum + m.over, 0);
	await tmgr.ui.setStatusBarItem(
		'overrun',
		total > 0
			? {
					text: `${duration(total)} over estimate`,
					tooltip: `${over.length} task${
						over.length === 1 ? '' : 's'
					} in progress went past the estimate`,
					command: 'tmgr.estimate.refresh',
			  }
			: null,
	);
};

const refresh = async () => {
	await updateStatusBar();
	await tmgr.ui.refresh('badges', 'ratio');
	await tmgr.ui.refresh('page', 'report');
	await tmgr.ui.refresh('section', 'estimate');
};

tmgr.ui.provideBadges('ratio', async (tasks) => {
	const threshold = await warnAt();
	const badges = {};
	for (const task of tasks) {
		const m = measure(task, threshold);
		if (m) {
			badges[task.id] = {
				text: `${Math.round(m.ratio * 100)}%`,
				color: m.color,
				tooltip: `${duration(m.spent)} of ${duration(m.estimate)} estimated`,
			};
		}
	}
	return badges;
});

tmgr.ui.providePage('report', async () => {
	const measured = await overrunTasks();
	const over = measured.filter(({ m }) => m.over > 0);
	const totalOver = over.reduce((sum, { m }) => sum + m.over, 0);
	return {
		type: 'stack',
		children: [
			{ type: 'heading', text: 'Estimate vs actual', level: 1 },
			{
				type: 'stack',
				direction: 'row',
				children: [
					{
						type: 'stat',
						label: 'Tasks with an estimate',
						value: String(measured.length),
					},
					{
						type: 'stat',
						label: 'Over the estimate',
						value: String(over.length),
						tone: over.length ? 'danger' : 'success',
					},
					{
						type: 'stat',
						label: 'Time over',
						value: duration(totalOver),
						tone: totalOver ? 'danger' : 'default',
					},
				],
			},
			measured.length
				? {
						type: 'table',
						columns: [
							{ key: 'task', title: 'Task' },
							{ key: 'spent', title: 'Spent' },
							{ key: 'estimate', title: 'Estimate' },
							{ key: 'ratio', title: 'Used' },
						],
						rows: measured.map(({ task, m }) => ({
							taskId: task.id,
							cells: {
								task: {
									type: 'taskLink',
									taskId: task.id,
									text: `${taskKey(task)} ${task.title}`,
								},
								spent: duration(m.spent),
								estimate: duration(m.estimate),
								ratio: {
									type: 'badge',
									text: `${Math.round(m.ratio * 100)}%`,
									color: m.color,
								},
							},
						})),
				  }
				: {
						type: 'text',
						text: 'No task has an estimate yet. Set one in the task panel to see it here.',
						tone: 'muted',
				  },
			{ type: 'button', text: 'Recalculate', command: 'tmgr.estimate.refresh' },
		],
	};
});

tmgr.ui.provideTaskSection('estimate', async (task) => {
	const m = measure(task, await warnAt());
	if (!m)
		return {
			type: 'text',
			text: 'No estimate yet. Set one to compare it with the tracked time.',
			tone: 'muted',
		};
	return {
		type: 'stack',
		children: [
			{
				type: 'stack',
				direction: 'row',
				children: [
					{ type: 'stat', label: 'Spent', value: duration(m.spent) },
					{ type: 'stat', label: 'Estimate', value: duration(m.estimate) },
					{
						type: 'stat',
						label: m.over > 0 ? 'Over' : 'Left',
						value: duration(m.over > 0 ? m.over : m.estimate - m.spent),
						tone: m.over > 0 ? 'danger' : 'success',
					},
				],
			},
			{ type: 'progress', value: m.ratio, color: m.color },
		],
	};
});

tmgr.commands.register('tmgr.estimate.refresh', refresh);
tmgr.events.on('timer.started', refresh);
tmgr.events.on('timer.stopped', refresh);
tmgr.events.on('task.updated', refresh);
tmgr.events.on('task.statusChanged', refresh);

updateStatusBar();
