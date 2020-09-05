import BaseLayout from "../components/Layouts/BaseLayout";
import Modal from "../components/Layouts/Modal";
import Navbar from "../components/UIElements/Navbar";
import Button from "../components/UIElements/Button";
import NewButton from "../components/UIElements/NewButton";
import DropdownMenu from "../components/UIElements/DropdownMenu";
import Countdown from "../components/Tasks/Countdown";
import InputField from "../components/UIElements/InputField";
import Loader from "../components/UIElements/Loader";
import DayNightSwitch from "../components/UIElements/DayNightSwitch";
import RockerSwitch from 'vue-rocker-switch';
import AccountDropdown from '@/components/UIElements/AccountDropdown';
import Alert from '@/components/UIElements/Alert';
import "vue-rocker-switch/dist/vue-rocker-switch.css";

export default [
    Alert,
    BaseLayout,
    RockerSwitch,
    DayNightSwitch,
    Modal,
    Navbar,
    DropdownMenu,
    Countdown,
    InputField,
    Button,
    NewButton,
    Loader,
    AccountDropdown
]
