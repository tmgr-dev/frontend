const inProgress = async () => {
	const statuses = await tmgr.statuses.list();
	const active = new Set(statuses.filter((s) => s.type === 'active').map((s) => s.id));
	const { items } = await tmgr.tasks.list({ perPage: 100 });
	return items.filter((task) => active.has(task.status_id));
};

const refresh = async () => {
	const tasks = await inProgress();
	await tmgr.ui.setStatusBarItem('count', { text: `${tasks.length} in progress`, command: 'yourname.hello.refresh' });
	await tmgr.ui.refresh('page', 'summary');
};

tmgr.commands.register('yourname.hello.refresh', refresh);

tmgr.ui.providePage('summary', async () => {
	const tasks = await inProgress();
	return {
		type: 'stack',
		children: [
			{ type: 'heading', text: 'Tasks in progress', level: 1 },
			{ type: 'list', items: tasks.map((task) => ({ type: 'taskLink', taskId: task.id, text: task.title })) },
			{ type: 'button', text: 'Refresh', command: 'yourname.hello.refresh' },
		],
	};
});

refresh();
