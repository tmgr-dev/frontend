import { t } from '@/i18n';

export interface ValidationResult {
	isValid: boolean;
	errors?: string[];
}

export const validateEmailString = (input: string): ValidationResult => {
	if (!input.trim()) {
		return {
			isValid: false,
			errors: [t('workspace.helpers.emails.empty')],
		};
	}

	const emails = input.split(',').map((email) => email.trim());

	const emailPattern = /^[a-zA-Z0-9._-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$/;

	const errors: string[] = [];

	emails.forEach((email, index) => {
		if (!email) {
			errors.push(
				t('workspace.helpers.emails.emptyAt', { position: index + 1 }),
			);
		} else if (!emailPattern.test(email)) {
			errors.push(t('workspace.helpers.emails.invalid', { email }));
		}
	});

	return {
		isValid: errors.length === 0,
		errors: errors.length > 0 ? errors : undefined,
	};
};
