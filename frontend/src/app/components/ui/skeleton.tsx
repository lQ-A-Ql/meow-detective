import { cn } from "./utils";

function Skeleton({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="skeleton"
      className={cn("bg-forensics-panel-strong rounded-none opacity-70", className)}
      {...props}
    />
  );
}

export { Skeleton };
