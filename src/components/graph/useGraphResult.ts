import { getGraphRelated } from '@/actions/tmgr/graph';
import type { GraphInclude } from '@/actions/tmgr/graphParams';
import type { GraphResult } from '@/types/graph';
import { onBeforeUnmount, ref, shallowRef, watch } from 'vue';

export interface GraphQuery {
	entity: string;
	depth: 1 | 2;
	include: GraphInclude[];
	workspace_id?: number | string | null;
	limit?: number;
}

export function useGraphResult(query: () => GraphQuery) {
	const result = shallowRef<GraphResult | null>(null);
	const loading = ref(false);
	const failed = ref(false);
	let seq = 0;

	const load = async () => {
		const current = ++seq;
		loading.value = true;
		failed.value = false;
		try {
			const data = await getGraphRelated(query());
			if (current === seq) result.value = data;
		} catch {
			if (current === seq) failed.value = true;
		} finally {
			if (current === seq) loading.value = false;
		}
	};

	watch(() => JSON.stringify(query()), load, { immediate: true });
	onBeforeUnmount(() => {
		seq++;
	});

	return { result, loading, failed, reload: load };
}
