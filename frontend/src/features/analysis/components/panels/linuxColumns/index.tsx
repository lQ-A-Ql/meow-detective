import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { buildCommandColumns, buildJournalColumns, buildLoginColumns } from './systemColumns';
import { buildCronColumns, buildPackageColumns, buildSudoColumns } from './packageColumns';
import {
  buildSystemConfigColumns,
  buildWebAccessColumns,
  buildWebErrorColumns,
  buildWebFindingColumns,
  buildWebSiteColumns,
} from './webColumns';
import { buildMysqlConfigColumns, buildMysqlFindingColumns, buildMysqlLogColumns } from './mysqlColumns';

// Column titles are translated, so all column arrays are memoized on `t`
// instead of being rebuilt on every render. Every column also declares a
// `text` accessor so the DenseDataTable filter bar can keyword-match rows;
// low-cardinality columns are marked `filterable` to get a dropdown.
export function useLinuxColumns() {
  const { t } = useTranslation();
  return useMemo(
    () => ({
      journal: buildJournalColumns(t),
      login: buildLoginColumns(t),
      command: buildCommandColumns(t),
      package: buildPackageColumns(t),
      cron: buildCronColumns(t),
      sudo: buildSudoColumns(t),
      systemConfig: buildSystemConfigColumns(t),
      webSite: buildWebSiteColumns(t),
      webAccess: buildWebAccessColumns(t),
      webError: buildWebErrorColumns(t),
      webFinding: buildWebFindingColumns(t),
      mysqlFinding: buildMysqlFindingColumns(t),
      mysqlConfig: buildMysqlConfigColumns(t),
      mysqlLog: buildMysqlLogColumns(t),
    }),
    [t],
  );
}
