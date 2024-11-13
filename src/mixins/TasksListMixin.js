export default {
	methods: {
		getTaskFormattedTime(task) {
			const taskTime = task instanceof Object ? task.common_time : task;
			return this.formatTime(taskTime);
		},
		formatTime(taskTime) {
			let hours = Math.floor(taskTime / 3600);
			let minutes = Math.ceil((taskTime % 3600) / 60);

			return `${
				hours > 0 ? hours + ' hour' + (hours > 1 ? 's' : '') : ''
			}  ${minutes} minute${minutes > 1 ? 's' : ''}`;
		},
	},
};
