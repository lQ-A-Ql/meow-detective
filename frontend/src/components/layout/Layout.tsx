import { Outlet } from "react-router";
import { ShellContainer } from "@/features/shell/containers/ShellContainer";

export function Layout() {
  return (
    <ShellContainer>
      <Outlet />
    </ShellContainer>
  );
}
