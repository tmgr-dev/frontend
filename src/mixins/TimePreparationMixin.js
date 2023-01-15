import moment from 'moment';

export default {
	methods: {
		secondsToHumanReadableString(seconds) {
			const { hours, minutes } = this.secondsToCountdownObject(seconds);
			return `${hours}:${minutes}`;
		},
		secondsToCountdownObject(seconds) {
			const second = seconds % 60;
			let minute = ((seconds - second) / 60) | 0;
			const hour = (minute / 60) | 0;
			minute = minute - hour * 60;

			const countdown = {};
			countdown.hours = this.prepareClockNumber(hour);
			countdown.minutes = this.prepareClockNumber(minute);
			countdown.seconds = this.prepareClockNumber(second);

			return countdown;
		},
		prepareClockNumber(num) {
			return num < 10 ? '0' + num : num;
		},
		formatDatetime(datetime) {
			return moment(datetime).format('DD.MM');
		},
	},
};
