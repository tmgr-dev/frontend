import { api } from './client';

// Daily routines (recurring tasks). Mirrors the web daily-routines endpoints.
export async function getRoutines(): Promise<any[]> {
	const { data } = await api.get('daily-routines/tasks');
	return data?.data ?? data ?? [];
}

export async function getRoutine(id: number): Promise<any> {
	const { data } = await api.get(`daily-routines/tasks/${id}`);
	return data?.data ?? data;
}

export async function createRoutine(payload: any): Promise<any> {
	const { data } = await api.post('daily-routines/tasks', payload);
	return data?.data ?? data;
}

export async function updateRoutine(id: number, payload: any): Promise<any> {
	const { data } = await api.put(`daily-routines/tasks/${id}`, payload);
	return data?.data ?? data;
}

export async function deleteRoutine(id: number): Promise<void> {
	await api.delete(`daily-routines/tasks/${id}`);
}

export async function completeRoutine(id: number): Promise<any> {
	const { data } = await api.post(`daily-routines/tasks/${id}/complete`);
	return data?.data ?? data;
}

export async function completeRoutineOn(id: number, date: string): Promise<any> {
	const { data } = await api.post(`daily-routines/tasks/${id}/complete-on`, {
		date,
	});
	return data?.data ?? data;
}

// Monthly view data: expanded routine instances between two dates.
export async function expandRoutines(from: string, to: string): Promise<any> {
	const { data } = await api.get('daily-routines/expand', {
		params: { from, to },
	});
	return data?.data ?? data;
}

// Yearly completion stats (for calendar heat/summary).
export async function routineStats(year: number): Promise<any> {
	const { data } = await api.get('daily-routines/expand/stats', {
		params: { year },
	});
	return data?.data ?? data;
}
