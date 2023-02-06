import BaseLayout from '../components/layouts/BaseLayout';
import Modal from '../components/Modal.vue';
import Button from '../components/UIElements/Button';
import NewButton from '../components/UIElements/NewButton';
import DropdownMenu from '../components/UIElements/DropdownMenu';
import Loader from '../components/loaders/Loader.vue';
import DayNightSwitch from '../components/UIElements/DayNightSwitch';
import AccountDropdown from 'src/components/UIElements/AccountDropdown';
import Alert from 'src/components/UIElements/Alert';

export default [
	Alert,
	BaseLayout,
	DayNightSwitch,
	Modal,
	DropdownMenu,
	Button,
	NewButton,
	Loader,
	AccountDropdown,
];
