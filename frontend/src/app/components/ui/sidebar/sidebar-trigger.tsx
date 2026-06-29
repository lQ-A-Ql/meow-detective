"use client";

import * as React from "react";
import { useTranslation } from 'react-i18next';
import { PanelLeftIcon } from "lucide-react";

import { cn } from "../utils";
import { Button } from "../button";
import { useSidebar } from "./sidebar-provider";

function SidebarTrigger({
  className,
  onClick,
  ...props
}: React.ComponentProps<typeof Button>) {
  const { t } = useTranslation();
  const { toggleSidebar } = useSidebar();

  return (
    <Button
      data-sidebar="trigger"
      data-slot="sidebar-trigger"
      variant="ghost"
      size="icon"
      className={cn("size-7", className)}
      onClick={(event) => {
        onClick?.(event);
        toggleSidebar();
      }}
      {...props}
    >
      <PanelLeftIcon />
      <span className="sr-only">{t('sidebar.toggleSidebar')}</span>
    </Button>
  );
}

export { SidebarTrigger };
