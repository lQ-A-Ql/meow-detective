// Archived UI prototype from 2026-05; not part of the production frontend.
const state = {
  currentPage: 'home',
  selectedFileId: 'f-001',
  selectedSearchHitId: 's-001',
  selectedTimelineId: 't-004',
  selectedArtifactFamily: 'Prefetch',
  selectedArtifactId: 'a-003',
  selectedViewerTab: 'metadata',
  drawerOpen: false,
};

const data = {
  nav: [
    { id: 'home', title: 'CASE HOME' },
    { id: 'files', title: 'FILE BROWSER' },
    { id: 'search', title: 'SEARCH' },
    { id: 'timeline', title: 'TIMELINE' },
    { id: 'artifacts', title: 'ARTIFACTS' },
    { id: 'reports', title: 'REPORTS' },
  ],
  subbars: {
    home: {
      left: ['CASE: FINCH', 'SOURCES: 3', 'INDEX: 82%'],
      right: ['TASKS: 3', 'CACHE: runtime.db warm', 'TRACE: 3 events'],
    },
    files: {
      left: ['SOURCE: FINCH-1.E01', 'PATH: Users/qaq/Desktop', 'FILTER: modified < 48h'],
      right: ['VIEWER: range-handle', 'CACHE: preview warm', 'ROWS: 5'],
    },
    search: {
      left: ['MODE: literal', 'SCOPE: Desktop + Documents', 'QUERY: credential OR wallet OR exfil'],
      right: ['HITS: 218', 'SNIPPETS: cached', 'SAVE QUERY'],
    },
    timeline: {
      left: ['RANGE: 2026-05-12', 'GRAIN: hour', 'SOURCE: Desktop/Recent'],
      right: ['PROJECTED: 62k', 'FILTER: pivots only', 'OPEN SOURCE'],
    },
    artifacts: {
      left: ['FAMILY: Prefetch', 'SOURCE: Windows/Prefetch', 'ROWS: 3'],
      right: ['LINK: timeline ready', 'REPORT: candidate', 'EXPORT'],
    },
    reports: {
      left: ['TEMPLATE: Investigation Brief', 'SCOPE: selected evidence', 'TARGET: HTML/JSON'],
      right: ['REPORTS: 3 recent', 'QUEUE: idle', 'TRACE: milestone pending'],
    },
  },
  summaryCells: [
    ['Mounted sources', '3', '1 E01 + 2 logical sets'],
    ['Indexed objects', '1.9k', 'text + metadata currently searchable'],
    ['High-signal hits', '218', 'search cluster on wallet / credential language'],
    ['Runtime cache', '14 handles', 'preview and timeline buckets held in runtime.db'],
  ],
  files: [
    { id: 'f-001', name: 'wallet-recovery-plan.txt', type: 'TEXT', modified: '09:14', size: '18 KB', deleted: 'N', path: 'Users/qaq/Desktop/wallet-recovery-plan.txt' },
    { id: 'f-002', name: 'invoice-may.msg', type: 'MAIL', modified: '21:47', size: '196 KB', deleted: 'N', path: 'Users/qaq/Downloads/invoice-may.msg' },
    { id: 'f-003', name: 'KeePass.lnk', type: 'LNK', modified: '09:17', size: '3 KB', deleted: 'N', path: 'Users/qaq/Recent/KeePass.lnk' },
    { id: 'f-004', name: 'draft-exfil-notes.docx', type: 'DOCX', modified: '10:22', size: '88 KB', deleted: 'N', path: 'Users/qaq/Documents/draft-exfil-notes.docx' },
    { id: 'f-005', name: '$I4R9UQ.doc', type: 'RECYCLE', modified: '10:29', size: '544 B', deleted: 'Y', path: '$Recycle.Bin/.../$I4R9UQ.doc' },
  ],
  fileTree: [
    { id: '1', title: 'FINCH-1.E01', sub: '3 volumes mounted' },
    { id: '2', title: 'Users/qaq', sub: 'recent activity hotspot', active: true },
    { id: '3', title: 'Desktop', sub: '5 focused files' },
    { id: '4', title: 'Downloads', sub: 'suspicious docs and mail' },
    { id: '5', title: 'AppData/Roaming', sub: 'browser and jump-list traces' },
  ],
  fileDetails: {
    'f-001': {
      title: 'wallet-recovery-plan.txt',
      fields: [
        ['PATH', 'Users/qaq/Desktop/wallet-recovery-plan.txt'],
        ['SHA-256', '5db8a34f2b90c44f9f12f08a3ef4d2acaf92'],
        ['CREATED', '2026-05-12T09:12:14Z'],
        ['MODIFIED', '2026-05-12T09:14:03Z'],
        ['RUNTIME HANDLE', 'open · expires in 08m'],
      ],
      metadata: ['ENTRY TYPE file', 'ENCODING UTF-8', 'INDEXED yes', 'ARTIFACT LINKS 3'],
      text: 'Recovery phrase staged for export. Suspect moved <mark>wallet</mark> seed into a temporary note before packaging archive. Mentions <mark>credential</mark> reuse and removing local traces after exfil.',
      hex: '0000  52 65 63 6f 76 65 72 79 20 70 68 72 61 73 65 20\n0010  73 74 61 67 65 64 20 66 6f 72 20 65 78 70 6f 72 74\n0020  2e 20 53 75 73 70 65 63 74 20 6d 6f 76 65 64 20 77',
      preview: 'Plain-text note preview rendered from chunk cache. Full file remains range-loaded.',
    },
    'f-002': {
      title: 'invoice-may.msg',
      fields: [
        ['PATH', 'Users/qaq/Downloads/invoice-may.msg'],
        ['SHA-256', 'b781ca48239dd8f1204ac8846f710c90cfe'],
        ['MODIFIED', '2026-05-11T21:47:51Z'],
        ['ATTACHMENTS', '2'],
        ['RUNTIME HANDLE', 'none'],
      ],
      metadata: ['MESSAGE CLASS IPM.Note', 'INDEXED partial', 'ARTIFACT LINKS 1'],
      text: 'Message body preview deferred. Use search snippets or attachment drill-down.',
      hex: '0000  d0 cf 11 e0 a1 b1 1a e1 00 00 00 00 00 00 00 00',
      preview: 'Mail preview placeholder.',
    },
    'f-003': {
      title: 'KeePass.lnk',
      fields: [
        ['PATH', 'Users/qaq/Recent/KeePass.lnk'],
        ['TARGET', 'C:/Program Files/KeePass/KeePass.exe'],
        ['MODIFIED', '2026-05-12T09:17:08Z'],
        ['ARTIFACT', 'LNK_ACTIVITY'],
        ['RUNTIME HANDLE', 'metadata-only'],
      ],
      metadata: ['ENTRY TYPE lnk', 'TIMELINE LINK yes', 'ARTIFACT LINKS 4'],
      text: 'Shortcut metadata only. Parsed shell link blocks are in artifact view.',
      hex: '0000  4c 00 00 00 01 14 02 00 00 00 00 00 c0 00 00 00',
      preview: 'Shell link preview placeholder.',
    },
  },
  searchHits: [
    { id: 's-001', path: 'Users/qaq/Desktop/wallet-recovery-plan.txt', mode: 'literal', score: '98.4', summary: 'High-signal plain-text note', snippet: 'Suspect moved <mark>wallet</mark> seed into a temporary note before packaging archive. Mentions <mark>credential</mark> reuse.' },
    { id: 's-002', path: 'Users/qaq/Documents/draft-exfil-notes.docx', mode: 'phrase', score: '92.7', summary: 'Draft planning language', snippet: 'Operator wrote “remove local traces after <mark>exfil</mark>” and staged the archive in Downloads.' },
    { id: 's-003', path: 'Users/qaq/AppData/Roaming/Browser/History', mode: 'regex', score: '71.6', summary: 'Browser history extraction', snippet: 'Multiple visits matched /credential|wallet|seed/ over a 48h window.' },
  ],
  searchDetails: {
    's-001': [['OBJECT', 'wallet-recovery-plan.txt'], ['READING', 'dense intentional human-authored note'], ['REPORT STATUS', 'candidate'], ['LINK', 'timeline + file browser']],
    's-002': [['OBJECT', 'draft-exfil-notes.docx'], ['READING', 'planning language around staging and cleanup'], ['REPORT STATUS', 'watch'], ['LINK', 'search cluster']],
    's-003': [['OBJECT', 'browser history extraction'], ['READING', 'behavioral pattern rather than single document'], ['REPORT STATUS', 'supporting'], ['LINK', 'artifact family']],
  },
  timeline: [
    { id: 't-001', ts: '08:21', type: 'FILE_CREATED', title: 'archive staging note created', h: 24, tone: 'dim' },
    { id: 't-002', ts: '08:44', type: 'PROGRAM_EXECUTION', title: 'KeePass executed', h: 52, tone: 'soft' },
    { id: 't-003', ts: '09:03', type: 'LINK_ACTIVITY', title: 'recent shortcut updated', h: 32, tone: 'dim' },
    { id: 't-004', ts: '09:14', type: 'FILE_MODIFIED', title: 'wallet-recovery-plan updated', h: 68, tone: 'active' },
    { id: 't-005', ts: '10:29', type: 'FILE_DELETED', title: 'draft removed into recycle bin', h: 44, tone: 'soft' },
    { id: 't-006', ts: '11:02', type: 'TAG_ADDED', title: 'analyst tag applied', h: 20, tone: 'dim' },
  ],
  timelineDetails: {
    't-004': [['EVENT', 'wallet-recovery-plan updated'], ['SOURCE', 'f-001'], ['PROJECTION', 'MACB + search cluster'], ['CONFIDENCE', 'high']],
  },
  artifactFamilies: ['Prefetch', 'LNK', 'Jump List', 'Recycle Bin', 'Registry', 'SRU'],
  artifacts: {
    Prefetch: [
      { id: 'a-001', title: 'KeePass.exe', summary: 'run count 14 · last run 09:12', source: 'Windows/Prefetch' },
      { id: 'a-002', title: '7zFM.exe', summary: 'run count 3 · archive path refs', source: 'Windows/Prefetch' },
      { id: 'a-003', title: 'cmd.exe', summary: 'run count 9 · user profile refs', source: 'Windows/Prefetch' },
    ],
    LNK: [{ id: 'a-101', title: 'KeePass.lnk', summary: 'recent shortcut to KeePass', source: 'Users/qaq/Recent' }],
    'Jump List': [{ id: 'a-201', title: 'Word automaticDestinations', summary: 'recovered 6 recent docs', source: 'AppData/Roaming' }],
    'Recycle Bin': [{ id: 'a-301', title: '$I4R9UQ.doc', summary: 'deleted 10:29 · original in Documents', source: '$Recycle.Bin' }],
    Registry: [{ id: 'a-401', title: 'RunMRU', summary: 'cmd /c archive.bat', source: 'NTUSER.DAT' }],
    SRU: [{ id: 'a-501', title: 'App resource usage', summary: 'KeePass + 7z overlap', source: 'SRUDB.dat' }],
  },
  artifactDetails: {
    'a-003': [['TITLE', 'cmd.exe · Prefetch'], ['RUN COUNT', '9'], ['LAST RUN', '2026-05-12T09:18:07Z'], ['TIMELINE LINK', 'PROGRAM_EXECUTION']],
  },
  reports: [
    ['Investigation Brief', 'narrative HTML summary'],
    ['Artifact Ledger', 'JSON / CSV structured export'],
    ['Executive Snapshot', 'short readable handoff'],
  ],
  recentReports: [
    ['FINCH-brief-2026-05-16.html', 'completed', '12 evidence objects + 4 timeline pivots'],
    ['artifact-ledger-prefetch.csv', 'ready', '18 Prefetch rows'],
    ['case-raw-summary.json', 'queued', 'waiting for latest projection'],
  ],
  jobs: [
    ['Index extracted text · DS-02', 'Searching 1,948/2,311 files', 78],
    ['Prefetch extractor · User volume', 'Normalizing execution events', 61],
    ['Timeline projection', 'Waiting for artifact pipeline', 18],
  ],
  warnings: [
    ['runtime.db', 'preview handle cache healthy'],
    ['search index', 'OCR backlog deferred'],
    ['data source', 'one E01 segment missing but readable'],
  ],
  trace: [
    '15:01:44Z doc.updated agent=claude-main design.md',
    '15:05:19Z schema.created runtime-cache migration staged',
    '15:18:06Z milestone.completed frontend prototype planning',
  ],
};

function q(sel, root = document) { return root.querySelector(sel); }
function qa(sel, root = document) { return [...root.querySelectorAll(sel)]; }

function button(label, active = false, cls = 'chip') {
  return `<button class="${cls}${active ? ' active' : ''}">${label}</button>`;
}

function renderNav() {
  q('#page-nav').innerHTML = data.nav.map(item =>
    `<button class="nav-item${state.currentPage === item.id ? ' active' : ''}" data-page="${item.id}">${item.title}</button>`
  ).join('');

  qa('.nav-item').forEach(node => {
    node.addEventListener('click', () => {
      state.currentPage = node.dataset.page;
      render();
    });
  });
}

function renderSubbar() {
  const cfg = data.subbars[state.currentPage];
  q('#subbar-left').innerHTML = cfg.left.map((v, i) => button(v, i === 0)).join('');
  q('#subbar-right').innerHTML = cfg.right.map(v => button(v, false, 'mode-btn')).join('');
}

function renderHome(page) {
  page.innerHTML = `
    <div class="page-grid">
      <div class="main-column">
        <div class="page-head">
          <div>
            <h1 class="page-title">CASE HOME</h1>
            <div class="page-copy">案件总览以线性区块和数据矩阵组织，而不是卡片。</div>
          </div>
          <div class="subbar-group">
            ${button('ACTIVE TASKS 3', true)}
            ${button('REPORT CANDIDATES 14')}
          </div>
        </div>

        <div class="summary-grid">
          ${data.summaryCells.map(([label, value, sub]) => `
            <div class="summary-cell">
              <label>${label}</label>
              <strong>${value}</strong>
              <div class="cell-sub">${sub}</div>
            </div>
          `).join('')}
        </div>

        <div class="line-block">
          <div class="split-topline">
            <div>
              <div class="mini-label">CURRENT POSTURE</div>
              <div class="section-title" style="margin-top:6px;font-size:18px;">工作台优先展示状态，而不是装饰。</div>
            </div>
            <div class="subbar-group">
              ${button('INDEX 82%', true)}
              ${button('ARTIFACTS 62K')}
              ${button('CACHE warm')}
            </div>
          </div>
          <div class="line-copy">首页职责是让分析者在几秒内知道当前案件、当前运行态、当前高价值对象和当前出口。</div>
        </div>

        <div class="table-wrap" style="min-height:0;flex:1;">
          <div class="split-topline">
            <div>
              <div class="mini-label">RECENT LANES</div>
              <div class="section-title" style="margin-top:6px;font-size:18px;">Recent tasks / pivots / output lanes</div>
            </div>
          </div>
          <div class="table-scroll">
            <table>
              <thead><tr><th>Lane</th><th>Summary</th><th>Status</th></tr></thead>
              <tbody>
                <tr><td>Indexer</td><td>Text extraction and tantivy ingest still active.</td><td>running</td></tr>
                <tr><td>Windows traces</td><td>Prefetch, LNK, Registry and Recycle Bin projections available.</td><td>ready</td></tr>
                <tr><td>Reporting</td><td>Brief + ledger export surfaces prepared.</td><td>stable</td></tr>
              </tbody>
            </table>
          </div>
        </div>
      </div>
      ${renderInspector('CASE INSPECTOR', 'FINCH', [
        ['CASE', '24-031'],
        ['EXAMINER', 'Qin Ao'],
        ['SOURCES', '3 mounted'],
        ['TRACE', '3 session events'],
        ['RUNTIME CACHE', '14 active handles'],
      ], ['OPEN SEARCH', 'OPEN REPORTS'])}
    </div>
  `;
}

function renderFiles(page) {
  const detail = data.fileDetails[state.selectedFileId];
  page.innerHTML = `
    <div class="page-grid">
      <div class="main-column">
        <div class="page-head">
          <div>
            <h1 class="page-title">FILE BROWSER</h1>
            <div class="page-copy">左侧树已降为页面内结构，全球导航改由顶部栏承担。</div>
          </div>
          <div class="subbar-group">
            ${button('RANGE VIEWER', true)}
            ${button('CACHE aware')}
          </div>
        </div>
        <div class="dense-split" style="flex:1;min-height:0;">
          <div class="dense-col panel">
            <div class="mini-label">SOURCE TREE</div>
            <ul class="tree-list" id="file-tree"></ul>
          </div>
          <div class="dense-col" style="display:grid;grid-template-rows:1fr 260px;min-height:0;">
            <div class="table-wrap" style="min-height:0;">
              <div class="split-topline">
                <div>
                  <div class="mini-label">DIRECTORY LEDGER</div>
                  <div class="section-title" style="margin-top:6px;font-size:18px;">Current path surface</div>
                </div>
                <div class="subbar-group">
                  ${button('modified < 48h', true)}
                  ${button('text + lnk + docx')}
                </div>
              </div>
              <div class="table-scroll">
                <table>
                  <thead><tr><th>Name</th><th>Type</th><th>Modified</th><th>Size</th><th>Deleted</th></tr></thead>
                  <tbody id="file-table"></tbody>
                </table>
              </div>
            </div>
            <div class="viewer-wrap">
              <div class="split-topline">
                <div>
                  <div class="mini-label">PREVIEW SURFACE</div>
                  <div class="section-title" style="margin-top:6px;font-size:18px;">Chunked inspection</div>
                </div>
                <div class="tab-row" id="viewer-tabs"></div>
              </div>
              <div class="viewer-body" id="viewer-body"></div>
            </div>
          </div>
        </div>
      </div>
      ${renderInspector('OBJECT INSPECTOR', detail.title, detail.fields, ['TAG', 'NOTE', 'REPORT'])}
    </div>
  `;

  q('#file-tree', page).innerHTML = data.fileTree.map(item => `
    <li class="tree-item${item.active ? ' active' : ''}">
      <div class="tree-title">${item.title}</div>
      <div class="faint mono">${item.sub}</div>
    </li>
  `).join('');

  q('#file-table', page).innerHTML = data.files.map(row => `
    <tr class="${state.selectedFileId === row.id ? 'active' : ''}" data-id="${row.id}">
      <td>${row.name}<div class="faint mono" style="margin-top:4px;">${row.path}</div></td>
      <td>${row.type}</td>
      <td class="mono">${row.modified}</td>
      <td>${row.size}</td>
      <td>${row.deleted}</td>
    </tr>
  `).join('');

  qa('#file-table tr', page).forEach(tr => tr.addEventListener('click', () => {
    state.selectedFileId = tr.dataset.id;
    renderPages();
  }));

  q('#viewer-tabs', page).innerHTML = ['metadata', 'text', 'hex', 'preview'].map(tab =>
    `<button class="tab-item${state.selectedViewerTab === tab ? ' active' : ''}" data-tab="${tab}">${tab.toUpperCase()}</button>`
  ).join('');
  qa('#viewer-tabs .tab-item', page).forEach(btn => btn.addEventListener('click', () => {
    state.selectedViewerTab = btn.dataset.tab;
    renderPages();
  }));

  if (state.selectedViewerTab === 'metadata') q('#viewer-body', page).innerHTML = detail.metadata.join('<br>');
  if (state.selectedViewerTab === 'text') q('#viewer-body', page).innerHTML = detail.text;
  if (state.selectedViewerTab === 'hex') q('#viewer-body', page).innerHTML = `<div class="hex">${detail.hex}</div>`;
  if (state.selectedViewerTab === 'preview') q('#viewer-body', page).innerHTML = detail.preview;
}

function renderSearch(page) {
  const detail = data.searchDetails[state.selectedSearchHitId];
  page.innerHTML = `
    <div class="page-grid">
      <div class="main-column">
        <div class="page-head">
          <div>
            <h1 class="page-title">SEARCH</h1>
            <div class="page-copy">搜索界面收敛为命令、过滤和结果账本，而非视觉卡片流。</div>
          </div>
          <div class="subbar-group">
            ${button('LITERAL', true)}
            ${button('PHRASE')}
            ${button('REGEX')}
          </div>
        </div>
        <div class="line-block">
          <div class="split-topline">
            <div>
              <div class="mini-label">QUERY</div>
              <div class="section-title" style="margin-top:6px;font-size:18px;">credential OR wallet OR exfil</div>
            </div>
            <div class="subbar-group">
              ${button('Desktop + Documents', true)}
              ${button('48h')}
              ${button('218 hits')}
            </div>
          </div>
        </div>
        <div class="dense-split" style="flex:1;min-height:0;grid-template-columns:300px 1fr;">
          <div class="dense-col panel">
            <div class="mini-label">SAVED LENSES</div>
            <ul class="saved-list">
              <li class="saved-item active"><div class="saved-title">wallet staging</div><div class="faint mono">high intent wording</div></li>
              <li class="saved-item"><div class="saved-title">archive batch files</div><div class="faint mono">packaging workflow</div></li>
              <li class="saved-item"><div class="saved-title">credential handling</div><div class="faint mono">secret material language</div></li>
            </ul>
          </div>
          <div class="dense-col" style="display:grid;grid-template-rows:1fr 200px;min-height:0;">
            <div class="table-wrap" style="min-height:0;">
              <div class="split-topline"><div><div class="mini-label">RESULT SET</div><div class="section-title" style="margin-top:6px;font-size:18px;">Search ledger</div></div></div>
              <div class="table-scroll">
                <table>
                  <thead><tr><th>Path</th><th>Mode</th><th>Score</th><th>Summary</th></tr></thead>
                  <tbody id="search-table"></tbody>
                </table>
              </div>
            </div>
            <div class="viewer-wrap">
              <div class="split-topline"><div><div class="mini-label">SNIPPET</div><div class="section-title" style="margin-top:6px;font-size:18px;">Context window</div></div></div>
              <div class="snippet-box" id="snippet-box"></div>
            </div>
          </div>
        </div>
      </div>
      ${renderInspector('HIT INSPECTOR', 'SEARCH HIT', detail, ['OPEN FILE', 'TO TIMELINE', 'TO REPORT'])}
    </div>
  `;

  q('#search-table', page).innerHTML = data.searchHits.map(row => `
    <tr class="${state.selectedSearchHitId === row.id ? 'active' : ''}" data-id="${row.id}">
      <td>${row.path}</td><td>${row.mode}</td><td>${row.score}</td><td>${row.summary}</td>
    </tr>
  `).join('');

  qa('#search-table tr', page).forEach(tr => tr.addEventListener('click', () => {
    state.selectedSearchHitId = tr.dataset.id;
    renderPages();
  }));

  q('#snippet-box', page).innerHTML = data.searchHits.find(x => x.id === state.selectedSearchHitId).snippet;
}

function renderTimeline(page) {
  const detail = data.timelineDetails[state.selectedTimelineId];
  page.innerHTML = `
    <div class="page-grid">
      <div class="main-column">
        <div class="page-head">
          <div>
            <h1 class="page-title">TIMELINE</h1>
            <div class="page-copy">时间表达收敛成条带、刻度和事件表，不依赖装饰图表。</div>
          </div>
          <div class="subbar-group">
            ${button('HOUR', true)}
            ${button('DAY')}
            ${button('WEEK')}
          </div>
        </div>
        <div class="viewer-wrap" style="min-height:170px;">
          <div class="split-topline"><div><div class="mini-label">TIME BAND</div><div class="section-title" style="margin-top:6px;font-size:18px;">Projected sequence</div></div></div>
          <div class="timeline-band" id="timeline-band"></div>
        </div>
        <div class="table-wrap" style="flex:1;min-height:0;">
          <div class="split-topline"><div><div class="mini-label">EVENT LEDGER</div><div class="section-title" style="margin-top:6px;font-size:18px;">Event rows</div></div></div>
          <div class="table-scroll">
            <table>
              <thead><tr><th>Time</th><th>Type</th><th>Title</th></tr></thead>
              <tbody id="timeline-table"></tbody>
            </table>
          </div>
        </div>
      </div>
      ${renderInspector('TIMELINE INSPECTOR', 'EVENT', detail, ['OPEN SOURCE', 'NOTE'])}
    </div>
  `;

  q('#timeline-band', page).innerHTML = data.timeline.map(item => `
    <div class="timeline-slot ${item.tone === 'active' ? '' : item.tone}">
      <div class="faint mono">${item.ts}</div>
      <div class="timeline-bar" style="height:${item.h}px;"></div>
      <div class="line-title">${item.type}</div>
    </div>
  `).join('');

  q('#timeline-table', page).innerHTML = data.timeline.map(item => `
    <tr class="${state.selectedTimelineId === item.id ? 'active' : ''}" data-id="${item.id}">
      <td class="mono">${item.ts}</td><td>${item.type}</td><td>${item.title}</td>
    </tr>
  `).join('');

  qa('#timeline-table tr', page).forEach(tr => tr.addEventListener('click', () => {
    state.selectedTimelineId = tr.dataset.id;
    renderPages();
  }));
}

function renderArtifacts(page) {
  const rows = data.artifacts[state.selectedArtifactFamily];
  const detail = data.artifactDetails[state.selectedArtifactId] || data.artifactDetails['a-003'];
  page.innerHTML = `
    <div class="page-grid">
      <div class="main-column">
        <div class="page-head">
          <div>
            <h1 class="page-title">ARTIFACTS</h1>
            <div class="page-copy">工件家族切换从侧栏迁移到顶部/次栏逻辑，主区域保持表格密度。</div>
          </div>
          <div class="subbar-group" id="artifact-tabs"></div>
        </div>
        <div class="table-wrap" style="flex:1;min-height:0;">
          <div class="split-topline"><div><div class="mini-label">ARTIFACT LEDGER</div><div class="section-title" style="margin-top:6px;font-size:18px;">${state.selectedArtifactFamily}</div></div></div>
          <div class="table-scroll">
            <table>
              <thead><tr><th>Title</th><th>Summary</th><th>Source</th></tr></thead>
              <tbody id="artifact-table"></tbody>
            </table>
          </div>
        </div>
        <div class="line-block">
          <div class="line-block-grid">
            <div class="line-cell"><div class="mini-label">EXECUTION CLUES</div><strong>18</strong></div>
            <div class="line-cell"><div class="mini-label">NAVIGATION CLUES</div><strong>44</strong></div>
            <div class="line-cell"><div class="mini-label">DELETION PIVOTS</div><strong>7</strong></div>
          </div>
        </div>
      </div>
      ${renderInspector('ARTIFACT INSPECTOR', state.selectedArtifactFamily, detail, ['TO TIMELINE', 'OPEN SOURCE', 'TO REPORT'])}
    </div>
  `;

  q('#artifact-tabs', page).innerHTML = data.artifactFamilies.map(name =>
    `<button class="tab-item${state.selectedArtifactFamily === name ? ' active' : ''}" data-family="${name}">${name.toUpperCase()}</button>`
  ).join('');
  qa('#artifact-tabs .tab-item', page).forEach(btn => btn.addEventListener('click', () => {
    state.selectedArtifactFamily = btn.dataset.family;
    state.selectedArtifactId = data.artifacts[state.selectedArtifactFamily][0].id;
    renderPages();
  }));

  q('#artifact-table', page).innerHTML = rows.map(row => `
    <tr class="${state.selectedArtifactId === row.id ? 'active' : ''}" data-id="${row.id}">
      <td>${row.title}</td><td>${row.summary}</td><td>${row.source}</td>
    </tr>
  `).join('');
  qa('#artifact-table tr', page).forEach(tr => tr.addEventListener('click', () => {
    state.selectedArtifactId = tr.dataset.id;
    renderPages();
  }));
}

function renderReports(page) {
  page.innerHTML = `
    <div class="page-grid">
      <div class="main-column">
        <div class="page-head">
          <div>
            <h1 class="page-title">REPORTS</h1>
            <div class="page-copy">报告页改成线性导出账本，不再以模板卡片展示。</div>
          </div>
          <div class="subbar-group">
            ${button('HTML BRIEF', true)}
            ${button('JSON LEDGER')}
            ${button('CSV EXPORT')}
          </div>
        </div>
        <div class="line-block">
          <div class="split-topline">
            <div>
              <div class="mini-label">EXPORT RANGE</div>
              <div class="section-title" style="margin-top:6px;font-size:18px;">Selected evidence, selected pivots, selected tags</div>
            </div>
            <div class="subbar-group">
              ${button('14 evidence', true)}
              ${button('4 pivots')}
              ${button('queue idle')}
            </div>
          </div>
        </div>
        <div class="table-wrap" style="flex:1;min-height:0;">
          <div class="split-topline"><div><div class="mini-label">RECENT OUTPUTS</div><div class="section-title" style="margin-top:6px;font-size:18px;">Report history</div></div></div>
          <div class="table-scroll">
            <table>
              <thead><tr><th>Artifact</th><th>Status</th><th>Note</th></tr></thead>
              <tbody>
                ${data.recentReports.map(row => `<tr><td>${row[0]}</td><td>${row[1]}</td><td>${row[2]}</td></tr>`).join('')}
              </tbody>
            </table>
          </div>
        </div>
      </div>
      ${renderInspector('REPORT INSPECTOR', 'INVESTIGATION BRIEF', [
        ['PRIMARY', 'HTML narrative summary'],
        ['SECONDARY', 'JSON / CSV structured ledger'],
        ['TRACE', 'milestone summary pending'],
        ['QUEUE', 'idle'],
      ], ['START EXPORT', 'PREVIEW SCOPE'])}
    </div>
  `;
}

function renderInspector(label, title, fields, actions) {
  return `
    <aside class="inspector">
      <div class="mini-label">${label}</div>
      <h3 style="margin-top:8px;font-size:22px;">${title}</h3>
      <div class="inspector-copy">右侧检查器保持高密度字段面板，不承担页面级视觉装饰任务。</div>
      <div class="fields">
        ${fields.map(([k, v]) => `
          <div class="field">
            <label>${k}</label>
            <code>${v}</code>
          </div>
        `).join('')}
      </div>
      <div class="mini-actions">
        ${actions.map(a => `<button class="action-btn">${a}</button>`).join('')}
      </div>
    </aside>
  `;
}

function renderPages() {
  qa('.page').forEach(page => {
    const active = page.dataset.page === state.currentPage;
    page.classList.toggle('active', active);
    if (!active) return;
    if (page.dataset.page === 'home') renderHome(page);
    if (page.dataset.page === 'files') renderFiles(page);
    if (page.dataset.page === 'search') renderSearch(page);
    if (page.dataset.page === 'timeline') renderTimeline(page);
    if (page.dataset.page === 'artifacts') renderArtifacts(page);
    if (page.dataset.page === 'reports') renderReports(page);
  });
}

function renderDrawer() {
  const drawer = q('#drawer');
  drawer.classList.toggle('open', state.drawerOpen);
  q('#drawer-toggle-secondary').textContent = state.drawerOpen ? 'COLLAPSE' : 'EXPAND';

  q('#job-list').innerHTML = data.jobs.map(([title, sub, pct]) => `
    <div class="job-row">
      <div class="split-topline"><div>${title}</div><div class="mono">${pct}%</div></div>
      <div class="faint">${sub}</div>
      <div class="progress"><span style="width:${pct}%"></span></div>
    </div>
  `).join('');

  q('#warn-list').innerHTML = data.warnings.map(([title, body]) => `
    <div class="warn-row">
      <div>${title}</div>
      <div class="faint">${body}</div>
    </div>
  `).join('');

  q('#trace-list').textContent = data.trace.join('\n');
}

function render() {
  renderNav();
  renderSubbar();
  renderPages();
  renderDrawer();
}

function bind() {
  q('#drawer-toggle').addEventListener('click', () => {
    state.drawerOpen = !state.drawerOpen;
    renderDrawer();
  });
  q('#drawer-toggle-secondary').addEventListener('click', () => {
    state.drawerOpen = !state.drawerOpen;
    renderDrawer();
  });
}

render();
bind();
