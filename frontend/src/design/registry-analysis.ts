import { Clock, Cpu, Database, HardDrive, Shield, Usb, Wifi } from 'lucide-react';

export const REGISTRY_ANALYSIS_TABS = [
  { key: 'users', labelKey: 'analysis.registry.tabs.users', icon: Shield },
  { key: 'hardware', labelKey: 'registryHardware.title', icon: Cpu },
  { key: 'activity', labelKey: 'analysis.registry.tabs.activity', icon: Clock },
  { key: 'network', labelKey: 'analysis.registry.tabs.network', icon: Wifi },
  { key: 'software', labelKey: 'analysis.registry.tabs.software', icon: Database },
  { key: 'usb', labelKey: 'analysis.registry.tabs.usb', icon: Usb },
  { key: 'raw', labelKey: 'analysis.registry.tabs.raw', icon: HardDrive },
] as const;
