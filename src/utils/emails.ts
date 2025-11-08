export interface ValidationResult {
	isValid: boolean;
	errors?: string[];
}

export const validateEmailString = (input: string): ValidationResult => {
	if (!input.trim()) {
		return {
			isValid: false,
			errors: ['Input string cannot be empty']
		};
	}

	const emails = input.split(',').map(email => email.trim());

	const emailPattern = /^[a-zA-Z0-9._-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$/;

	const errors: string[] = [];

	emails.forEach((email, index) => {
		if (!email) {
			errors.push(`Empty email at position ${index + 1}`);
		} else if (!emailPattern.test(email)) {
			errors.push(`Invalid email format: ${email}`);
		}
	});

	return {
		isValid: errors.length === 0,
		errors: errors.length > 0 ? errors : undefined
	};
};
