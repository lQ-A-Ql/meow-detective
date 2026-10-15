import { useHexPreviewModel, type HexPreviewProps } from '@/hooks/use-hex-preview-model';
import { HexPreviewContentView } from '../HexPreviewContentView';
export function HexPreviewContent(props: HexPreviewProps) { return <HexPreviewContentView model={useHexPreviewModel(props)} />; }
