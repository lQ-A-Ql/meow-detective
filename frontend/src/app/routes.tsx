import { createBrowserRouter } from "react-router";
import { Layout } from "@/components/layout/Layout";
import { CaseHome } from "./pages/CaseHome";
import { FileBrowser } from "./pages/FileBrowser";
import { Search } from "./pages/Search";
import { Timeline } from "./pages/Timeline";
import { Artifacts } from "./pages/Artifacts";
import { Reports } from "./pages/Reports";

import { Settings } from "./pages/Settings";

export const router = createBrowserRouter([
  {
    path: "/",
    Component: Layout,
    children: [
      { index: true, Component: CaseHome },
      { path: "files", Component: FileBrowser },
      { path: "search", Component: Search },
      { path: "timeline", Component: Timeline },
      { path: "artifacts", Component: Artifacts },
      { path: "reports", Component: Reports },
      { path: "settings", Component: Settings },
    ],
  },
]);
