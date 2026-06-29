import "@testing-library/jest-dom/vitest";
import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import {
  SidebarProvider,
  Sidebar,
  SidebarTrigger,
  SidebarContent,
} from "./sidebar";

describe("sidebar", () => {
  beforeEach(() => {
    Object.defineProperty(window, "matchMedia", {
      writable: true,
      value: vi.fn().mockImplementation((query: string) => ({
        matches: false,
        media: query,
        onchange: null,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        dispatchEvent: vi.fn(),
      })),
    });
  });

  it("renders SidebarProvider without crashing", () => {
    render(
      <SidebarProvider>
        <div data-testid="child">content</div>
      </SidebarProvider>,
    );

    expect(screen.getByTestId("child")).toBeDefined();
  });

  it("renders Sidebar and SidebarTrigger without crashing", () => {
    render(
      <SidebarProvider>
        <Sidebar>
          <SidebarContent>
            <span>menu</span>
          </SidebarContent>
        </Sidebar>
        <SidebarTrigger data-testid="trigger" />
      </SidebarProvider>,
    );

    expect(screen.getByText("menu")).toBeDefined();
    expect(screen.getByTestId("trigger")).toBeDefined();
  });
});
