import { FontAwesomeIcon, type FontAwesomeIconProps } from '@fortawesome/react-fontawesome';
import type { IconDefinition } from '@fortawesome/fontawesome-svg-core';
import type { ComponentType } from 'react';
import {
  faArrowDown,
  faArrowUp,
  faBackwardStep,
  faBookmark,
  faBox,
  faBoxArchive,
  faBoxesStacked,
  faBullseye,
  faChartColumn,
  faCheck,
  faChevronDown,
  faChevronLeft,
  faChevronRight,
  faChevronUp,
  faCircleCheck,
  faCircleDot,
  faCircleExclamation,
  faCircleNotch,
  faCircleQuestion,
  faCircleUser,
  faClock,
  faClockRotateLeft,
  faCodeBranch,
  faComment,
  faCommentDots,
  faCompactDisc,
  faCompress,
  faDatabase,
  faDisplay,
  faDownload,
  faEnvelope,
  faExpand,
  faEye,
  faEyeSlash,
  faFaceSmile,
  faFile,
  faFileAudio,
  faFileCircleCheck,
  faFileCircleExclamation,
  faFileCircleXmark,
  faFileCode,
  faFileLines,
  faFileVideo,
  faFilter,
  faFolder,
  faFolderOpen,
  faForwardStep,
  faGauge,
  faGear,
  faGlobe,
  faHardDrive,
  faImage,
  faImages,
  faKey,
  faLayerGroup,
  faLink,
  faLinkSlash,
  faLock,
  faMagnifyingGlass,
  faMagnifyingGlassMinus,
  faMagnifyingGlassPlus,
  faMemory,
  faMicrochip,
  faMicrophone,
  faMinus,
  faMobileScreenButton,
  faMusic,
  faNetworkWired,
  faPaperclip,
  faPause,
  faPen,
  faPlay,
  faPlug,
  faPlus,
  faPowerOff,
  faPuzzlePiece,
  faRotate,
  faRotateLeft,
  faRotateRight,
  faServer,
  faShield,
  faShieldHalved,
  faSort,
  faSpinner,
  faTerminal,
  faTrashCan,
  faTriangleExclamation,
  faUserCheck,
  faUserXmark,
  faVideo,
  faVolumeHigh,
  faVolumeXmark,
  faWaveSquare,
  faWeightHanging,
  faWifi,
  faWindowMaximize,
  faXmark,
} from '@fortawesome/free-solid-svg-icons';

type IconProps = Omit<FontAwesomeIconProps, 'icon' | 'size' | 'ref'> & {
  size?: number | string;
};

export type IconComponent = ComponentType<IconProps>;

function createIcon(definition: IconDefinition): IconComponent {
  return function Icon({ size, style, ...props }) {
    return (
      <FontAwesomeIcon
        icon={definition}
        {...props}
        style={size === undefined ? style : { width: size, height: size, ...style }}
      />
    );
  };
}

export const Activity = createIcon(faWaveSquare);
export const AlertCircle = createIcon(faCircleExclamation);
export const AlertTriangle = createIcon(faTriangleExclamation);
export const AppWindow = createIcon(faWindowMaximize);
export const Archive = createIcon(faBoxArchive);
export const ArrowDown = createIcon(faArrowDown);
export const ArrowUp = createIcon(faArrowUp);
export const ArrowUpDown = createIcon(faSort);
export const BarChart3 = createIcon(faChartColumn);
export const Bookmark = createIcon(faBookmark);
export const Boxes = createIcon(faBoxesStacked);
export const Check = createIcon(faCheck);
export const CheckCircle2 = createIcon(faCircleCheck);
export const CheckIcon = Check;
export const ChevronDown = createIcon(faChevronDown);
export const ChevronDownIcon = ChevronDown;
export const ChevronLeft = createIcon(faChevronLeft);
export const ChevronRight = createIcon(faChevronRight);
export const ChevronUp = createIcon(faChevronUp);
export const ChevronUpIcon = ChevronUp;
export const CircleAlert = createIcon(faCircleExclamation);
export const CircleCheck = createIcon(faCircleCheck);
export const CircleDashed = createIcon(faCircleNotch);
export const CircleDot = createIcon(faCircleDot);
export const CircuitBoard = createIcon(faMicrochip);
export const Clock = createIcon(faClock);
export const Clock3 = createIcon(faClock);
export const ContactRound = createIcon(faCircleUser);
export const Container = createIcon(faBox);
export const Cpu = createIcon(faMicrochip);
export const Database = createIcon(faDatabase);
export const Disc3 = createIcon(faCompactDisc);
export const Download = createIcon(faDownload);
export const Eye = createIcon(faEye);
export const EyeOff = createIcon(faEyeSlash);
export const File = createIcon(faFile);
export const FileAudio = createIcon(faFileAudio);
export const FileCheck2 = createIcon(faFileCircleCheck);
export const FileClock = createIcon(faFileLines);
export const FileCode = createIcon(faFileCode);
export const FileSearch = createIcon(faMagnifyingGlass);
export const FileText = createIcon(faFileLines);
export const FileVideo = createIcon(faFileVideo);
export const FileWarning = createIcon(faFileCircleExclamation);
export const FileX2 = createIcon(faFileCircleXmark);
export const Folder = createIcon(faFolder);
export const FolderOpen = createIcon(faFolderOpen);
export const Gauge = createIcon(faGauge);
export const GitBranch = createIcon(faCodeBranch);
export const Globe = createIcon(faGlobe);
export const HardDrive = createIcon(faHardDrive);
export const HardDriveDownload = createIcon(faHardDrive);
export const HelpCircle = createIcon(faCircleQuestion);
export const History = createIcon(faClockRotateLeft);
export const Image = createIcon(faImage);
export const ImageOff = createIcon(faImage);
export const Images = createIcon(faImages);
export const KeyRound = createIcon(faKey);
export const Layers = createIcon(faLayerGroup);
export const Link = createIcon(faLink);
export const Link2 = createIcon(faLink);
export const Link2Off = createIcon(faLinkSlash);
export const ListFilter = createIcon(faFilter);
export const LoaderCircle = createIcon(faSpinner);
export const Lock = createIcon(faLock);
export const Mail = createIcon(faEnvelope);
export const Maximize = createIcon(faExpand);
export const MemoryStick = createIcon(faMemory);
export const MessageCircle = createIcon(faComment);
export const MessageSquare = createIcon(faCommentDots);
export const Mic = createIcon(faMicrophone);
export const Microchip = createIcon(faMicrochip);
export const Minimize = createIcon(faCompress);
export const Minus = createIcon(faMinus);
export const Monitor = createIcon(faDisplay);
export const Music = createIcon(faMusic);
export const Network = createIcon(faNetworkWired);
export const Paperclip = createIcon(faPaperclip);
export const Pause = createIcon(faPause);
export const PencilLine = createIcon(faPen);
export const Play = createIcon(faPlay);
export const Plus = createIcon(faPlus);
export const Power = createIcon(faPowerOff);
export const Puzzle = createIcon(faPuzzlePiece);
export const RefreshCw = createIcon(faRotate);
export const RotateCcw = createIcon(faRotateLeft);
export const RotateCw = createIcon(faRotateRight);
export const Search = createIcon(faMagnifyingGlass);
export const Server = createIcon(faServer);
export const ServerCog = createIcon(faServer);
export const Settings = createIcon(faGear);
export const Shield = createIcon(faShield);
export const ShieldAlert = createIcon(faTriangleExclamation);
export const ShieldCheck = createIcon(faShieldHalved);
export const SkipBack = createIcon(faBackwardStep);
export const SkipForward = createIcon(faForwardStep);
export const Smartphone = createIcon(faMobileScreenButton);
export const Smile = createIcon(faFaceSmile);
export const Target = createIcon(faBullseye);
export const Terminal = createIcon(faTerminal);
export const Trash2 = createIcon(faTrashCan);
export const TriangleAlert = createIcon(faTriangleExclamation);
export const Usb = createIcon(faPlug);
export const UserRoundCheck = createIcon(faUserCheck);
export const UserRoundX = createIcon(faUserXmark);
export const Video = createIcon(faVideo);
export const Volume2 = createIcon(faVolumeHigh);
export const VolumeX = createIcon(faVolumeXmark);
export const Weight = createIcon(faWeightHanging);
export const Wifi = createIcon(faWifi);
export const X = createIcon(faXmark);
export const ZoomIn = createIcon(faMagnifyingGlassPlus);
export const ZoomOut = createIcon(faMagnifyingGlassMinus);
