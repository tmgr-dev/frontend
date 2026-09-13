import {
	archiveDailyTask,
	completeRoutineOn,
	convertRoutineToTask,
	deleteDailyTask,
	deleteDailyTaskInstance,
	downloadRoutinesIcs,
	expandRoutineRange,
	expandRoutineYearStats,
	importRoutinesIcs,
	quickCreateRoutine,
	rescheduleRoutineInstance,
	updateDailyTask,
} from '@/actions/tmgr/daily-tasks';

// Request ownership belongs to each store instance and each operation.
const requests = new WeakMap();
const contextKey = ({ rootState, rootGetters }) =>
	JSON.stringify([
		rootState?.user?.id,
		rootState?.sessionGeneration,
		rootGetters?.currentWorkspaceId,
	]);
function beginRequest(context, operation) {
	let operations = requests.get(context.state);
	if (!operations) requests.set(context.state, (operations = {}));
	const token = Symbol(operation);
	operations[operation] = token;
	const key = contextKey(context);
	return {
		owns: () => requests.get(context.state)?.[operation] === token,
		current: () =>
			requests.get(context.state)?.[operation] === token &&
			contextKey(context) === key,
	};
}

function fmtDate(d) {
	const y = d.getFullYear();
	const m = String(d.getMonth() + 1).padStart(2, '0');
	const day = String(d.getDate()).padStart(2, '0');
	return `${y}-${m}-${day}`;
}

const dailyRoutinesModule = {
	namespaced: true,
	state: () => ({
		view: 'list',
		cursor: new Date().toISOString(),
		entries: [],
		yearStats: {},
		isLoading: false,
		rangeLoading: false,
		yearLoading: false,
		rangeError: null,
		yearError: null,
		lastRange: { from: null, to: null },
	}),

	mutations: {
		reset(state) {
			requests.delete(state);
			state.entries = [];
			state.yearStats = {};
			state.lastRange = { from: null, to: null };
			state.rangeLoading = state.yearLoading = state.isLoading = false;
			state.rangeError = state.yearError = null;
		},
		setOperationLoading(state, { operation, loading }) {
			state[`${operation}Loading`] = loading;
			state.isLoading = state.rangeLoading || state.yearLoading;
		},
		setOperationError(state, { operation, error }) {
			state[`${operation}Error`] = error;
		},
		setView(state, view) {
			state.view = view;
		},
		setCursor(state, dateIso) {
			state.cursor = dateIso;
		},
		setEntries(state, entries) {
			state.entries = entries;
		},
		setLastRange(state, { from, to }) {
			state.lastRange = { from, to };
		},
		setYearStats(state, stats) {
			state.yearStats = stats;
		},
		setLoading(state, v) {
			state.isLoading = v;
		},
		patchEntry(state, { taskId, date, patch }) {
			state.entries = state.entries.map((e) => {
				if (e.task_id === taskId && e.date === date) {
					return { ...e, ...patch };
				}
				return e;
			});
		},
		removeEntries(state, predicate) {
			state.entries = state.entries.filter((e) => !predicate(e));
		},
	},

	actions: {
		async loadRange(context, { from, to }) {
			const { commit } = context;
			const request = beginRequest(context, 'range');
			commit('setOperationLoading', { operation: 'range', loading: true });
			commit('setOperationError', { operation: 'range', error: null });
			try {
				const data = await expandRoutineRange(from, to);
				if (!request.current()) return;
				commit('setEntries', data);
				commit('setLastRange', { from, to });
			} catch (error) {
				if (!request.current()) return;
				commit('setOperationError', {
					operation: 'range',
					error: error.message ?? 'Could not load routines',
				});
				throw error;
			} finally {
				if (request.owns())
					commit('setOperationLoading', { operation: 'range', loading: false });
			}
		},
		async loadYearStats(context, year) {
			const { commit } = context;
			const request = beginRequest(context, 'year');
			commit('setOperationLoading', { operation: 'year', loading: true });
			commit('setOperationError', { operation: 'year', error: null });
			try {
				const data = await expandRoutineYearStats(year);
				if (request.current()) commit('setYearStats', data);
			} catch (error) {
				if (!request.current()) return;
				commit('setOperationError', {
					operation: 'year',
					error: error.message ?? 'Could not load routine statistics',
				});
				throw error;
			} finally {
				if (request.owns())
					commit('setOperationLoading', { operation: 'year', loading: false });
			}
		},
		async toggleComplete({ commit, state, dispatch }, entry) {
			const result = await completeRoutineOn(entry.task_id, entry.date);
			commit('patchEntry', {
				taskId: entry.task_id,
				date: entry.date,
				patch: {
					completed: result.completed,
					instance_id: result.instance_id,
					status: result.status,
					virtual: false,
				},
			});
		},
		async reschedule({ dispatch, state }, { entry, newDate, newTime }) {
			const time = newTime || entry.time || '09:00';
			const scheduledFor = `${newDate} ${
				time.length === 5 ? time + ':00' : time
			}`;
			const instanceId = entry.instance_id ?? 'virtual';
			await rescheduleRoutineInstance(entry.task_id, instanceId, scheduledFor);
			if (state.lastRange.from && state.lastRange.to) {
				await dispatch('loadRange', state.lastRange);
			}
		},
		async moveRoutine(
			{ dispatch, state, rootState },
			{ entry, date, timeH, timeM, allDay },
		) {
			const isRecurring = entry.frequency && entry.frequency !== 'NONE';
			if (allDay && isRecurring) {
				return;
			}
			if (isRecurring) {
				const time =
					timeH != null
						? `${String(timeH).padStart(2, '0')}:${String(timeM ?? 0).padStart(
								2,
								'0',
						  )}`
						: entry.time || '09:00';
				const scheduledFor = `${date} ${
					time.length === 5 ? time + ':00' : time
				}`;
				const instanceId = entry.instance_id ?? 'virtual';
				await rescheduleRoutineInstance(
					entry.task_id,
					instanceId,
					scheduledFor,
				);
			} else {
				const payload = {
					title: entry.title,
					description: entry.description || '',
					user_id: rootState?.user?.id,
					is_daily_routine: true,
					is_recurring: false,
					routine_category:
						entry.routine_category?.id ?? entry.routine_category ?? 'none',
				};
				if (allDay) {
					payload.scheduled_date = date;
					payload.scheduled_time = null;
				} else if (timeH != null) {
					payload.scheduled_date = date;
					payload.scheduled_time = { hours: timeH, minutes: timeM ?? 0 };
				} else {
					payload.scheduled_date = date;
					if (entry.time) {
						const [h, m] = entry.time.split(':').map(Number);
						payload.scheduled_time = { hours: h, minutes: m };
					} else {
						payload.scheduled_time = null;
					}
				}
				await updateDailyTask(entry.task_id, payload);
			}
			if (state.lastRange.from && state.lastRange.to) {
				await dispatch('loadRange', state.lastRange);
			}
		},
		async quickCreate({ dispatch, state }, payload) {
			await quickCreateRoutine(payload);
			if (state.lastRange.from && state.lastRange.to) {
				await dispatch('loadRange', state.lastRange);
			}
		},
		async importIcs({ dispatch, state }, { file, mode }) {
			const result = await importRoutinesIcs(file, mode);
			if (state.lastRange.from && state.lastRange.to) {
				await dispatch('loadRange', state.lastRange);
			}
			return result;
		},
		async exportIcs(_ctx, ids) {
			const blob = await downloadRoutinesIcs(ids);
			const url = window.URL.createObjectURL(blob);
			const a = document.createElement('a');
			a.href = url;
			a.download = `tmgr-routines-${fmtDate(new Date())}.ics`;
			document.body.appendChild(a);
			a.click();
			a.remove();
			window.URL.revokeObjectURL(url);
		},
		async deleteRoutine({ commit }, taskId) {
			await deleteDailyTask(taskId);
			commit('removeEntries', (e) => e.task_id === taskId);
		},
		async convertRoutine(
			{ commit },
			{ taskId, workspaceId, projectCategoryId },
		) {
			const task = await convertRoutineToTask(taskId, {
				workspace_id: workspaceId,
				project_category_id: projectCategoryId ?? null,
			});
			commit('removeEntries', (e) => e.task_id === taskId);
			return task;
		},
		async archiveRoutine({ commit }, taskId) {
			await archiveDailyTask(taskId);
			commit('removeEntries', (e) => e.task_id === taskId);
		},
		async archiveDoneUnscheduled({ commit, state }) {
			const ids = [
				...new Set(
					state.entries
						.filter((e) => e.frequency === 'NONE' && e.completed)
						.map((e) => e.task_id),
				),
			];
			for (const id of ids) {
				await archiveDailyTask(id);
			}
			commit('removeEntries', (e) => ids.includes(e.task_id));
			return ids.length;
		},
		async deleteInstance({ commit }, { taskId, instanceId, date }) {
			await deleteDailyTaskInstance(taskId, instanceId);
			commit('patchEntry', {
				taskId,
				date,
				patch: {
					instance_id: null,
					virtual: true,
					completed: false,
					status: null,
				},
			});
		},
	},

	getters: {
		entriesByDate: (state) => (date) =>
			state.entries.filter((e) => e.date === date),
		entriesByTaskId: (state) => (taskId) =>
			state.entries.filter((e) => e.task_id === taskId),
	},
};

// Mutations started in another user/workspace/range must not patch the visible list.
for (const [name, action] of Object.entries(dailyRoutinesModule.actions)) {
	if (['loadRange', 'loadYearStats', 'exportIcs'].includes(name)) continue;
	dailyRoutinesModule.actions[name] = async (context, payload) => {
		const key = contextKey(context);
		const rangeRequest = requests.get(context.state)?.range;
		const current = () =>
			contextKey(context) === key &&
			requests.get(context.state)?.range === rangeRequest;
		return action(
			{
				...context,
				commit: (...args) => {
					if (current()) return context.commit(...args);
				},
				dispatch: (...args) => {
					if (current()) return context.dispatch(...args);
				},
			},
			payload,
		);
	};
}

export default dailyRoutinesModule;
