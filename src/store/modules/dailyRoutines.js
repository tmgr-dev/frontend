import {
	expandRoutineRange,
	expandRoutineYearStats,
	completeRoutineOn,
	rescheduleRoutineInstance,
	quickCreateRoutine,
	importRoutinesIcs,
	downloadRoutinesIcs,
	deleteDailyTask,
	deleteDailyTaskInstance,
	completeDailyTaskInstance,
	archiveDailyTask,
} from '@/actions/tmgr/daily-tasks';

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
		lastRange: { from: null, to: null },
	}),

	mutations: {
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
			state.entries = state.entries.map(e => {
				if (e.task_id === taskId && e.date === date) {
					return { ...e, ...patch };
				}
				return e;
			});
		},
		removeEntries(state, predicate) {
			state.entries = state.entries.filter(e => !predicate(e));
		},
	},

	actions: {
		async loadRange({ commit }, { from, to }) {
			commit('setLoading', true);
			try {
				const data = await expandRoutineRange(from, to);
				commit('setEntries', data);
				commit('setLastRange', { from, to });
			} finally {
				commit('setLoading', false);
			}
		},
		async loadYearStats({ commit }, year) {
			commit('setLoading', true);
			try {
				const data = await expandRoutineYearStats(year);
				commit('setYearStats', data);
			} finally {
				commit('setLoading', false);
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
			const scheduledFor = `${newDate} ${time.length === 5 ? time + ':00' : time}`;
			const instanceId = entry.instance_id ?? 'virtual';
			await rescheduleRoutineInstance(entry.task_id, instanceId, scheduledFor);
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
			commit('removeEntries', e => e.task_id === taskId);
		},
		async archiveRoutine({ commit }, taskId) {
			await archiveDailyTask(taskId);
			commit('removeEntries', e => e.task_id === taskId);
		},
		async archiveDoneUnscheduled({ commit, state }) {
			const ids = [
				...new Set(
					state.entries
						.filter(e => e.frequency === 'NONE' && e.completed)
						.map(e => e.task_id),
				),
			];
			for (const id of ids) {
				await archiveDailyTask(id);
			}
			commit('removeEntries', e => ids.includes(e.task_id));
			return ids.length;
		},
		async deleteInstance({ commit }, { taskId, instanceId, date }) {
			await deleteDailyTaskInstance(taskId, instanceId);
			commit('patchEntry', {
				taskId,
				date,
				patch: { instance_id: null, virtual: true, completed: false, status: null },
			});
		},
	},

	getters: {
		entriesByDate: state => date => state.entries.filter(e => e.date === date),
		entriesByTaskId: state => taskId => state.entries.filter(e => e.task_id === taskId),
	},
};

export default dailyRoutinesModule;
