export default function getTimeInSeconds(time) {
	const timeSplit = time.split(':').map(v => parseInt(v))
	return timeSplit[0] * 3600 + timeSplit[1] * 60
}
