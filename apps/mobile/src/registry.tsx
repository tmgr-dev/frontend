import { ReactElement } from 'react';
import { Splash } from './screens/auth/Splash';
import { Login } from './screens/auth/Login';
import { Register } from './screens/auth/Register';
import { SocialConnecting } from './screens/auth/SocialConnecting';
import { Forgot } from './screens/auth/Forgot';
import { Reset } from './screens/auth/Reset';
import { Swatch } from './screens/Swatch';
import { Dashboard } from './screens/core/Dashboard';
import { WorkspaceSwitcher } from './screens/core/WorkspaceSwitcher';
import { More } from './screens/core/More';
import { Settings } from './screens/core/Settings';
import { Placeholder, PLACEHOLDERS } from './screens/core/Placeholder';
import { Board } from './screens/board/Board';
import { List } from './screens/board/List';

export const registry: Record<string, ReactElement> = {
	swatch: <Swatch />,
	splash: <Splash />,

	'core.workspace-switcher': <WorkspaceSwitcher />,
	'core.dashboard.default': <Dashboard />,
	'core.dashboard.loading': <Dashboard loading />,
	'core.dashboard.empty': <Dashboard empty />,
	'core.list.empty': <Placeholder cfg={PLACEHOLDERS.list} />,
	'core.list.loading': <Placeholder cfg={PLACEHOLDERS.list} loading />,
	'core.routines.empty': <Placeholder cfg={PLACEHOLDERS.routines} />,
	'core.categories.empty': <Placeholder cfg={PLACEHOLDERS.categories} />,
	'core.archive.empty': <Placeholder cfg={PLACEHOLDERS.archive} />,
	'core.more': <More />,
	'core.settings': <Settings />,

	'board.default': <Board />,
	'board.loading': <Board loading />,
	'board.empty': <Board empty />,
	'board.filtered': <Board filter="Product" />,
	'list.default': <List />,
	'list.loading': <List loading />,
	'list.empty': <List empty />,


	'login.default': <Login />,
	'login.filled': <Login filled />,
	'login.loading': <Login filled loading />,
	'login.error': <Login error />,

	'register.default': <Register />,
	'register.validation': <Register validation />,
	'register.loading': <Register loading />,

	'social.connecting': <SocialConnecting provider="google" />,
	'social.error': <SocialConnecting provider="google" error />,

	'forgot.default': <Forgot />,
	'forgot.success': <Forgot success />,
	'forgot.error': <Forgot error />,

	'reset.default': <Reset />,
	'reset.success': <Reset success />,
	'reset.error': <Reset error />,
};

export const registryKeys = Object.keys(registry);
