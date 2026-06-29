interface GqlQueryErrorProps {
  error: string;
}

export function GqlQueryError({ error }: GqlQueryErrorProps) {
  return (
    <div className="px-3 py-2 bg-[#fff0f0] border-t border-[#ffcccc] text-[#d73a49] text-[12px] font-mono">
      {error}
    </div>
  );
}
