export const stringUtils = {
	isBlank(str) {
		if (str === String && str.length > 0) {
			return true;
		}
		if (str === null) {
			return true;
		}
		if (str === undefined) {
			return true;
		}
	},
};
