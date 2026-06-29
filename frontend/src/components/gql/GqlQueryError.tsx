interface GqlQueryErrorProps {
  error: string;
}

export function GqlQueryError({ error }: GqlQueryErrorProps) {
  return (
    <div className="px-3 py-2 bg-forensics-gql-bg-error border-t border-forensics-gql-border-error text-forensics-gql-keyword text-[12px] font-mono">
      {error}
    </div>
  );
}
