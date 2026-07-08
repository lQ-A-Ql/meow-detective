import { KeyValueField } from '@/components/data-display';

type ArtifactFieldProps = {
  label: string;
  value: string;
};

export function ArtifactField({ label, value }: ArtifactFieldProps) {
  return <KeyValueField label={label} value={value} />;
}
