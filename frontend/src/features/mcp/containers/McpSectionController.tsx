import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { McpSectionView } from '../components/McpSectionView';
import { McpServerItemController } from './McpServerItemController';
import { McpServerDialogController } from './McpServerDialogController';
import type { McpServerDialogProps } from '../model/mcp-server-dialog';
import type { McpServer } from '@/stores/mcp-types';

export interface McpSectionControllerProps {
  servers: McpServer[]; selectedServerId: string | null; loading: boolean; error: string | null;
  onAdd: McpServerDialogProps['onAdd']; onConnect: (id: string) => void; onDisconnect: (id: string) => void; onRemove: (id: string) => void; onSelect: (id: string) => void; testConnection: McpServerDialogProps['testConnection']; resourceList?: React.ReactNode; toolList?: React.ReactNode; hostToolList?: React.ReactNode;
}
export function McpSectionController(props: McpSectionControllerProps) {
 const { t } = useTranslation(); const [expanded,setExpanded]=useState(false); const [dialog,setDialog]=useState(false);
 return <McpSectionView t={t} expanded={expanded} loading={props.loading} onToggle={()=>setExpanded(v=>!v)} onAdd={()=>setDialog(true)} servers={props.servers.map(server=>({...server,component:McpServerItemController}))} selectedServerId={props.selectedServerId} onSelect={props.onSelect} onConnect={props.onConnect} onDisconnect={props.onDisconnect} onRemove={props.onRemove} hostToolList={props.hostToolList} resourceList={props.resourceList} toolList={props.toolList} error={props.error} dialog={dialog?<McpServerDialogController onClose={()=>setDialog(false)} onAdd={props.onAdd} testConnection={props.testConnection}/>:null}/>;
}
