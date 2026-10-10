import { formatDuration } from '@/utils/formatDuration';

export default {
	methods: {
		getTaskFormattedTime(task) {
			const taskTime = task instanceof Object ? task.common_time : task;
			return this.formatTime(taskTime);
		},
		formatTime(taskTime) {
			return formatDuration(taskTime);
		},
	},
};
