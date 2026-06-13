# Forensics Workbench V2 闀挎湡鎵ц寮€鍙戣鍒?
## 1. 鎽樿

V2 鐨勭洰鏍囦笉鏄户缁爢鍔熻兘鐐癸紝鑰屾槸鎶婂綋鍓嶅凡缁忔垚褰㈢殑 Windows 鍙栬瘉閾捐矾鍋氭垚涓€濂楀彲楠岃瘉銆佸彲瑙ｉ噴銆佸彲鍥炲綊銆佸彲鍙戝竷鐨勪骇鍝佸寲鑳藉姏锛屽苟鍦ㄦ鍩虹涓婅ˉ榻愯法宸ヤ欢鍏宠仈鍒嗘瀽銆佽妯″寲绋冲畾鎬у拰瀹夊叏娌荤悊闂幆銆?
鏈鍒掍互浠ヤ笅鐜扮姸涓哄熀绾匡細

- V1 鐨勪富閾捐矾宸插叿澶囨浠躲€佸鍏ャ€佹枃浠舵祻瑙堛€佹悳绱€佹椂闂寸嚎銆乄indows artifact銆佹姤鍛娿€丮CP 鐨勫熀鏈兘鍔?- `docs/stage-5-risk-register-remediation-status.md` 涓殑娈嬩綑椋庨櫓浠嶇劧瀛樺湪锛屽挨鍏舵槸澶ф牱鏈獙璇併€佺湡瀹?E01銆乄ebView2 濯掍綋 seek銆丷egistry 骞胯鐩栧拰渚濊禆娌荤悊
- `docs/parser-support-matrix.md` 涓?`docs/known-unsupported-formats.md` 宸茬粡鏄庣‘褰撳墠鑳藉姏杈圭晫锛屼絾灏氭湭褰㈡垚鍙戝竷绾у彲淇¤鏄?- `docs/mcp-security-model.md` 涓?`docs/export-and-media-safety.md` 宸茬粡寤虹珛鍩虹瀹夊叏杈圭晫锛屼絾瀹¤璁板綍銆佽瘎鍒嗗崱鍜屽彂甯冮棬绂佷粛闇€瑕佺郴缁熷寲

V2 缁х画閬靛畧杩欎簺纭害鏉燂細

- Windows-primary
- desktop-first
- single-user
- 鏃?HTTP server
- 鍓嶅悗绔粎閫氳繃 Tauri commands / events 閫氫俊
- `crates/transport` 鏄敮涓€璺ㄧ濂戠害婧?- 璇佹嵁鍙
- 鍓嶇鏂板 UI 鍙娇鐢ㄦ垨鍒涘缓鍏湁缁勪欢
- 鏂囨。銆乫ixture 璇存槑銆乥enchmark 璁板綍缁熶竴閲囩敤 UTF-8

## 2. 鍏抽敭鎺ュ彛涓庢枃妗ｅ琛?
### 2.1 璁″垝鏂板鎴栨墿灞曠殑鍏叡濂戠害

V2 鏈熼棿鏂板鎴栨墿灞曚互涓?6 缁?DTO / 濂戠害锛?
1. 鍙俊楠岃瘉 DTO
   - fixture 娓呭崟
   - expected JSON 鐗堟湰
   - 楠岃瘉缁撴灉
   - 宸紓鎽樿
2. Parser 鏀寔鐭╅樀 DTO
   - 鑳藉姏鐘舵€?   - 宸查獙璇佹牱鏈?   - 淇濊瘉绾у埆
   - 宸茬煡闄愬埗
3. 閿欒鍒嗙被 DTO
   - 閿欒鐮?   - 灞傜骇
   - 涓ラ噸搴?   - 鑴辨晱绛栫暐
   - 鏄惁鍙噸璇?4. Benchmark DTO
   - 鍦烘櫙
   - 鏁版嵁闆嗙瓑绾?   - 鍐?鐑€楁椂
   - 鍐呭瓨宄板€?   - 瀹夸富閰嶇疆
5. 鍏宠仈鍒嗘瀽 DTO
   - correlation node
   - edge
   - cluster
   - lead
   - confidence
   - provenance
6. 瀹夊叏娌荤悊 DTO
   - 瀵煎嚭瀹¤璁板綍
   - MCP 浼氳瘽绛栫暐
   - 濯掍綋鍙ユ焺绛栫暐
   - 鍙戝竷鏍搁獙缁撴灉

### 2.2 鏂囨。浣撶郴鏉冨▉鍏ュ彛

V2 浠ユ湰鏂囨。浣滀负闀挎湡鎵ц涓昏鍒掞紝骞剁敱浠ヤ笅涓撻鏂囨。鎵挎帴瀹炵幇缁嗚妭锛?
- `docs/fixture-handbook.md`
- `docs/expected-json-contract.md`
- `docs/error-classification-manual.md`
- `docs/benchmark-baseline.md`
- `docs/correlation-analysis-design.md`
- `docs/release-scorecard.md`

涓庣幇鏈夋枃妗ｇ殑鍏崇郴锛?
- `docs/validation-trust-framework.md`锛氬彲淇￠獙璇佷綋绯绘€昏
- `docs/parser-support-matrix.md`锛氬綋鍓嶈兘鍔涘熀绾夸笌 V2 鐩爣
- `docs/known-unsupported-formats.md`锛氭槑纭笉鎵胯杈圭晫
- `docs/mcp-security-model.md`锛歁CP 鏉冮檺涓庡璁℃ā鍨?- `docs/export-and-media-safety.md`锛氬鍑恒€佹彁鍙栥€佸獟浣撳崗璁畨鍏ㄨ竟鐣?
## 3. Stage Design

### Stage V2-1锛氬彲淇￠獙璇佷綋绯讳骇鍝佸寲

#### 鐩爣

鎶娾€滆В鏋愯繃鈥濇彁鍗囦负鈥滃湪鍝簺鏍锋湰涓婇獙璇佽繃銆佽緭鍑轰笌浠€涔堝熀鍑嗗榻愩€佸摢浜涘瓧娈典笉淇濊瘉鈥濄€?
#### 闃舵杈圭晫

- 鏍稿績瑕嗙洊閾捐矾锛欵01銆丷AW銆丯TFS銆丳refetch銆丩NK銆丷egistry銆丷ecycle Bin
- Browser History銆丒mail 杩涘叆鍙俊浣撶郴锛屼絾鏈樁娈靛彧鍋氬埌 smoke + medium fixture锛屼粛鏄庣‘鏍囨敞涓?Experimental
- 涓嶆壙璇?PST/OST/mbox 鍏ㄩ噺鏀寔锛汦mail 缁х画浠?EML/EMLX 缁撴瀯鍖栦俊鎭负涓?- 涓嶅仛瀹屾暣 Registry 娴忚鍣紝鍙仛褰撳墠瑙ｆ瀽閾捐矾鐨勫彲淇℃牎鍑?
#### Phase Tasks

1. Fixture 鍒嗗眰涓庢牱鏈洰褰?   - 寤虹珛 `public-small / public-medium / private-real-regression` 涓夊眰鏍锋湰浣撶郴
   - 姣忎釜鏍锋湰寮哄埗鎼哄甫鏉ユ簮璇存槑銆佸悎娉曟€ц鏄庛€丼HA-256銆侀鏈熻兘鍔涜鐩栥€佹晱鎰熷瓧娈佃鏄?   - 涓?E01銆丯TFS銆丳refetch銆丩NK銆丷egistry銆丷ecycle Bin 鍒朵綔鏈€灏忓彲鍏紑鏍锋湰
   - Browser / Email 琛?smoke 鏍锋湰涓?medium 鏍锋湰璁″垝
2. Expected JSON 涓庡榻愯鍒?   - 缁熶竴 expected JSON 缁撴瀯銆佸瓧娈靛懡鍚嶃€佹椂鍖哄綊涓€鍖栥€佽矾寰勮鑼冦€佺┖鍊肩瓥鐣ャ€佹帓搴忚鍒?   - 涓烘瘡鏉℃牳蹇冮摼璺缓绔?`Guaranteed / Best-effort / Not-guaranteed` 瀛楁鍒嗙骇
   - 寤虹珛宸紓姣斿瑙勫垯锛氱粨鏋勫樊寮傘€佸€煎樊寮傘€佸厑璁告紓绉诲瓧娈?3. 鏀寔鐭╅樀銆侀敊璇垎绫汇€佽嚜鍔ㄥ洖褰?   - 灏嗘敮鎸佺煩闃典笌宸茬煡涓嶆敮鎸佹牸寮忔敼涓虹敱鏍锋湰楠岃瘉缁撴灉椹卞姩鏇存柊
   - 寤虹珛 Parser / Image / Filesystem / Artifact / Persistence / Transport / UI / Security 閿欒鍒嗙被
   - CI 涓姞鍏ユ牳蹇?fixture 鍥炲綊銆乪xpected JSON 瀵规瘮銆佹枃妗ｄ竴鑷存€ф牎楠?4. 鐪熷疄鏍锋湰鍥炲綊璇存槑浜у搧鍖?   - 涓烘瘡涓牳蹇?parser 澧炲姞鐪熷疄鏍锋湰鍥炲綊璇存槑
   - 瀵?E01銆丯TFS銆丳refetch銆丩NK銆丷egistry銆丷ecycle Bin 褰㈡垚 investigator 鍙鐨勯獙璇佹憳瑕?
#### 闃舵楠屾敹鏍囧噯

- E01銆丯TFS銆丳refetch銆丩NK銆丷egistry銆丷ecycle Bin 鍧囧叿澶?public-small 涓庤嚦灏戜竴涓?public-medium fixture
- 姣忔潯鏍稿績閾捐矾鍧囨湁 expected JSON銆佸瓧娈典繚璇佺骇鍒€佺湡瀹炴牱鏈洖褰掕鏄?- `docs/parser-support-matrix.md`銆乣docs/known-unsupported-formats.md`銆乣docs/error-taxonomy.md` 閮借兘杩芥函鍒版牱鏈拰缁撴灉锛岃€屼笉鏄函鍙ｅ緞鎻忚堪
- Browser / Email 鑷冲皯鍏峰 smoke fixture锛屽苟鏄庣‘浠嶅睘 Experimental

### Stage V2-2锛氬宸ヤ欢鍏宠仈鍒嗘瀽涓庤皟鏌ュ伐浣滄祦

#### 鐩爣

鎶婂垎鏁ｇ殑 artifact 瑙ｆ瀽缁撴灉鏀舵潫鎴愬彲杩借釜銆佸彲瑙ｉ噴銆佸彲瀵煎嚭鐨勮皟鏌ョ嚎绱€?
#### 闃舵杈圭晫

- 鍏宠仈鍒嗘瀽浠呰鐩?Windows 涓婚摼璺細NTFS銆丳refetch銆丩NK銆丷egistry銆丷ecycle Bin銆丅rowser History銆丒ML/EMLX
- 涓嶅仛鍏ㄨ嚜鍔ㄥ畾缃紱鍙敓鎴愮嚎绱€佽瘉鎹叧绯讳笌缃俊搴︼紝涓嶈緭鍑衡€滅粨璁哄瀷鍒ゅ畾鈥?- 涓嶅紩鍏ユ柊鐨勭嫭绔嬫湇鍔★紱缁х画鐢卞悗绔湇鍔″眰鑱氬悎锛屽墠绔秷璐?DTO

#### Phase Tasks

1. 缁熶竴鍏宠仈妯″瀷
   - 瀹氫箟 correlation node / edge / cluster / lead 鐨勭粺涓€妯″瀷
   - 缁熶竴 provenance 缁撴瀯锛氭潵婧愬伐浠躲€佹簮璁板綍 ID銆佹彁鍙栨椂闂淬€佽В鏋愮増鏈€佸瓧娈典繚璇佺骇鍒?   - 瀹氫箟 confidence 鍥涙。锛歚Direct / Strong / Weak / Heuristic`
2. 鏍稿績鍏宠仈瑙勫垯钀藉湴
   - Prefetch 鈫?鍙墽琛屾枃浠惰矾寰?鈫?鏂囦欢绯荤粺鏉＄洰
   - LNK 鈫?鐩爣璺緞 / 鍗蜂俊鎭?鈫?鏂囦欢绯荤粺鏉＄洰
   - Registry Run / RecentDocs / UserAssist 鈫?鏂囦欢璺緞 鈫?鏃堕棿绾?   - Recycle Bin 鈫?宸插垹闄ゆ枃浠?鈫?鍘熻矾寰?/ 鍒犻櫎鏃堕棿
   - Browser 涓嬭浇璁板綍 鈫?鏂囦欢绯荤粺鏉＄洰 鈫?LNK / Prefetch
   - Email 闄勪欢鍚?/ 涓婚 / 鏃堕棿 鈫?鏂囦欢绯荤粺鏉＄洰 鈫?鏃堕棿绾?3. 璋冩煡宸ヤ綔娴佷笌鍓嶇浜や簰
   - 鏂板绾跨储鎬昏銆佽瘉鎹叧绯昏鍥俱€佹寜 lead drill-down 鐨勮鎯呴〉
   - Timeline銆丄rtifacts銆丗ile Browser銆丷eports 鍏变韩鍚屼竴濂楀叧鑱旂粨鏋滀笌 provenance 鏂囨
   - 鍏宠仈瑙嗗浘鐘舵€併€佺瓫閫変笌 drill-down 缁勪欢娌夋穩涓哄叕鏈夌粍浠?4. 鎶ュ憡涓庤В閲婁竴鑷存€?   - 鎶ュ憡瀵煎嚭鏂板鈥滃叧鑱斿垎鏋愮珷鑺傗€?   - 鎵€鏈夊叧鑱旂粨鏋滈兘蹇呴』鑳藉洖璺冲埌鍘熷 artifact / file / timeline 琛?   - 涓?3 涓湡瀹炲洖褰掓渚嬪缓绔?investigator walkthrough

#### 褰撳墠钀藉湴杩涘害锛?026-06-12锛?
- 宸插畬鎴?V2-2 绗竴鏉′骇鍝佸唴鍙閾捐矾锛?  - `CorrelationSnapshotDto`
  - `correlation_service`
  - `get_correlation_snapshot`
  - `CorrelationWorkspace`
  - `lead.matchSignals`
- 宸插畬鎴?V2-1 / V2-4 鐨勭涓€鐗堜骇鍝佸唴娌荤悊鍙閾捐矾锛?  - `V2GovernanceSnapshotDto`
  - `get_v2_governance_snapshot`
  - `/v2 -> V2GovernancePanels`
  - `supportMatrixEntries`
  - `errorTaxonomyEntries`
- 宸插畬鎴愰鐗堟姤鍛婂鍑哄鐢細
  - HTML `Correlation Leads`
  - JSON `correlation`
  - CSV 鍏宠仈鎽樿杩藉姞
- 褰撳墠瑙勫垯鑼冨洿宸插崌绾у埌鈥滄渶灏忕湡瀹炶鍒欓棴鐜€濓細
  - Artifact 鈫?File
  - Timeline 鈫?File
  - Artifact 鈫?Timeline锛坰hared `sourceObjectId`锛?  - `LNK.target_path -> File.path`
  - `BrowserDownload.targetPath -> File.path`
  - `BrowserHistory.url/title + visitTime -> timeline proximity signal`
  - `RegistryValue.data -> File.path / File.name`
  - `RecycleBin.original_path -> deleted File.path`
  - `Prefetch.executable -> File.name`
  - `EmailMessage.attachments[] -> File.name`
  - `JumpList.target_path -> File.path`
  - 瑙勫垯鍛戒腑鐨勭洰鏍囨枃浠惰嫢宸叉湁 timeline 浜嬩欢锛屼細鑷姩琛ユ寕 `TemporalContext`
  - BrowserDownload 宸插紑濮嬭ˉ鍏?24 灏忔椂绐楀彛鍐呯殑閭昏繎 timeline signal
  - BrowserHistory 宸插紑濮嬭ˉ鍏?`visitTime + url / title` 鐨勯偦杩?timeline signal
  - EmailMessage 宸插紑濮嬭ˉ鍏?`sentAt + subject / attachments` 鐨勯偦杩?timeline signal
- 褰撳墠 HTML 鎶ュ憡宸蹭粠鍗曡鎽樿鎻愬崌涓虹粨鏋勫寲 `Correlation Lead Details`锛屼細鍗曠嫭灞曠ず confidence銆乸rimary file銆乻upporting nodes銆乵atch signals銆乸rovenance 涓?caveats
- 灏氭湭瀹屾垚鐨勮鍒欎粛鎸夋湰 Stage 鍘熻鍒掓帹杩涳紝涓嶅簲灏嗗綋鍓嶉鐗堣涓?V2-2 鍏ㄩ噺瀹屾垚

#### 闃舵楠屾敹鏍囧噯

- 鑷冲皯 6 绫绘牳蹇冨叧鑱旇鍒欒惤鍦板苟鍙洖婧?provenance
- 绾跨储瑙嗗浘銆乼imeline銆乤rtifact 璇︽儏銆佹姤鍛婂鍑轰娇鐢ㄥ悓涓€濂楀叧鑱旂粨鏋?- 鑷冲皯 3 涓湡瀹炴渚?walkthrough 鑳界ǔ瀹氬鐜板悓涓€璋冩煡璺緞
- 浠讳竴 lead 閮借兘璇存槑鏉ユ簮銆佸尮閰嶄緷鎹€佺疆淇″害鍜屾湭淇濊瘉瀛楁

### Stage V2-3锛氭€ц兘銆佽妯′笌绋冲畾鎬ч獙璇?

#### 当前补充落地进度（2026-06-13）

- `CorrelationLeadDto` 与 `CorrelationClusterDto` 已新增 `families[]`，规则家族归属不再只停留在 `familyCoverage[]` 聚合层。
- 后端关联服务已按 artifact type 结构化派生 `families[]`，`familyCoverage[]` 优先基于结构化字段统计，provenance / signal 文本仅作为兼容兜底。
- 前端 `CorrelationWorkspace` 已在 lead、cluster 与选中详情中展示规则家族，`summarizeLeadKinds()` 优先使用 `lead.families[]`。
- 报告导出已在 JSON、HTML 结构化 lead 详情和文本摘要中输出规则家族，便于发布审查与 investigator 复核。
#### 鐩爣

纭繚鐪熷疄澶?case 涓嬫枃浠舵爲銆佹悳绱€佹椂闂寸嚎銆乤rtifact 鎻愬彇銆佸彇娑堟搷浣滈兘鍙娴嬩笖涓嶄細鎶婁骇鍝佹嫋鍨€?
#### 闃舵杈圭晫

- 缁х画淇濇寔鍗曟満妗岄潰妯″紡锛屼笉鍋氬垎甯冨紡澶勭悊鍜屼簯浠诲姟缂栨帓
- 涓嶈拷姹傛墍鏈夎矾寰勭粷瀵规渶蹇紝浼樺厛寤虹珛鍙鐜?benchmark 涓庡洖褰掗棬妲?- 澶ч暅鍍忎笌鎱㈡祴鎸?`PR 鏍稿績鍥炲綊 + 瀹氭椂鍏ㄩ噺鍥炲綊 + 鎵嬪伐鐪熷疄鏍锋湰鍥炲綊` 鍒嗗眰杩愯

#### Phase Tasks

1. Benchmark 浣撶郴涓庢暟鎹垎绾?   - 寤虹珛 `Small / Medium / Large` 鏁版嵁闆嗗畾涔夈€佸涓绘満鍩虹嚎閰嶇疆銆佸喎/鐑繍琛屽彛寰?   - benchmark 瑕嗙洊瀵煎叆銆佹枃浠舵爲灞曞紑銆佹枃浠跺垎椤点€佹悳绱㈡煡璇€佹椂闂寸嚎杩囨护銆佹牳蹇?artifact 鎻愬彇銆佹姤鍛婂鍑恒€佸彇娑堜换鍔?   - 杈撳嚭 benchmark snapshot 涓庣増鏈瘮瀵圭粨鏋?2. 鐑偣璺緞鏀跺彛
   - 浼樺寲鏂囦欢鏍戞噿鍔犺浇銆佸垎椤垫帓搴忎竴鑷存€с€佹悳绱㈢储寮?warm path銆佹椂闂寸嚎鑱氬悎銆乤rtifact 鎵归噺鍐欏簱
   - 涓洪暱浠诲姟寤虹珛缁熶竴 progress銆乧ancel銆乸artial-result 绛栫暐
   - 娓呯悊楂樿€﹀悎 orchestrator / mod.rs 涓婂笣鏂囦欢涓Θ纰嶆€ц兘瑙傛祴涓庢祴璇曠殑缁撴瀯鍊?3. 绋冲畾鎬т笌璧勬簮杈圭晫
   - 澧炲姞闀挎椂杩愯銆侀噸澶嶅鍏ャ€侀噸澶嶆煡璇€佸紓甯镐腑鏂仮澶嶃€侀儴鍒嗗け璐ユ仮澶嶆祴璇?   - 寤虹珛鍐呭瓨宄板€笺€佸彞鏌勬硠婕忋€佹暟鎹簱澧為暱銆佺紦瀛樺け鏁堜笌娓呯悊妫€鏌?   - 灏嗘參娴嬫媶鍒嗕负 nightly / release candidate / 鎵嬪伐鐪熷疄鏍锋湰鍥炲綊涓夊眰
4. 鎬ц兘闂ㄧ鎺ュ叆鍙戝竷
   - benchmark 鍥炲綊瓒呰繃闃堝€煎嵆闃绘柇鍚堝苟鎴栭樆鏂€欓€夊彂甯?   - 杈撳嚭 investigator 鍙鐨勬€ц兘璇存槑锛氶€傜敤瑙勬ā銆佸凡楠岃瘉涓婇檺銆佷粛鏈壙璇轰笂闄?
#### 闃舵楠屾敹鏍囧噯

- benchmark 鍦ㄥ浐瀹氬涓婚厤缃笂鍙ǔ瀹氶噸澶嶏紝杩炵画 3 娆″亸宸湪鍙帴鍙楀尯闂村唴
- medium / large 鏁版嵁闆嗚揪鍒版棦瀹氭€ц兘闃堝€?- 闀夸换鍔″彇娑堛€侀儴鍒嗗け璐ユ仮澶嶃€侀噸澶嶅鍏?/ 鏌ヨ銆乶ightly 鎱㈡祴鍏ㄩ儴鎺ュ叆鑷姩鍖?- 鏂囦欢鏍戙€佹悳绱€佹椂闂寸嚎銆乤rtifact 鎻愬彇涓嶅瓨鍦ㄥ凡鐭?P0 / P1 绾ф€ц兘闃绘柇

### Stage V2-4锛氬畨鍏ㄦ不鐞嗕笌鍙戝竷娌荤悊

#### 鐩爣

鎶婂彇璇佸伐鍏锋渶鏁忔劅鐨勫閮ㄨ竟鐣屾敹绱ф垚鈥滈粯璁ゅ畨鍏ㄣ€佸彲瀹¤銆佸彲闃绘柇鍙戝竷鈥濄€?
#### 闃舵杈圭晫

- 涓嶅仛缁勭粐绾у绉熸埛鏉冮檺绯荤粺锛涚户缁互鍗曠敤鎴锋闈负鍓嶆彁
- 閲嶇偣鏀剁揣锛氬鍑鸿矾寰勩€乷verwrite銆丮CP stdio/SSE銆佸獟浣?handle銆侀敊璇劚鏁忋€佸璁¤褰曘€佸彂甯冮棬绂?- 涓嶆斁瀹藉凡鏈夊畨鍏ㄧ害鏉燂紱V2 鍙細鏇翠弗鏍?
#### Phase Tasks

1. 鏉冮檺妯″瀷涓庡璁¤褰?   - 鏄庣‘ MCP 鏉冮檺妯″瀷锛歳esource銆乼ool銆乸rompt銆乶etwork 鍥涚被鏉冮檺鐨勯粯璁ゅ€笺€佸崌绾ф潯浠躲€佸璁″瓧娈?   - 瀵煎嚭銆佽В鍘嬨€佸獟浣撹闂€丮CP 杩炴帴閮藉啓鍏ョ粺涓€瀹¤璁板綍
   - 瀹¤璁板綍鑷冲皯鍖呭惈锛氭椂闂淬€佹搷浣滆€呫€乧ase銆佸姩浣溿€佺洰鏍囥€佺瓥鐣ャ€佺粨鏋溿€侀敊璇爜
2. 杈圭晫瀹炵幇鏀剁揣
   - 瀵煎嚭璺緞瑙勮寖鍖栥€侀槻绌胯秺銆侀槻瑕嗙洊榛樿銆佺洰鏍囧瓨鍦ㄦ椂鏄惧紡纭
   - SSE 浠呭厑璁?http/https銆佺姝?embedded credentials锛泂tdio 浠呭厑璁稿懡浠ゅ悕鐧藉悕鍗曪紝涓嶅厑璁歌矾寰勬垨绌哄瓧鑺?   - 濯掍綋 handle 蹇呴』鐭敓鍛藉懆鏈熴€佷笉鍙硠闇茬墿鐞嗚矾寰勩€佷笉鍙法 case 澶嶇敤
   - 閿欒杈撳嚭鎸夐敊璇垎绫荤粺涓€鑴辨晱锛岀姝㈠師濮嬬郴缁熻矾寰勩€佸嚟鎹€佺幆澧冨彉閲忕洿鍑哄墠绔?3. 鍙戝竷娌荤悊涓庨槻婕傜Щ
   - 灏嗘敮鎸佺煩闃点€佸凡鐭ヤ笉鏀寔鏍煎紡銆侀敊璇垎绫汇€乥enchmark 鍩虹嚎銆佸畨鍏ㄦā鍨嬬撼鍏?release gate
   - 寤虹珛鏂囨。婕傜Щ妫€鏌ワ細瀹炵幇鍙樻洿瑙﹀彂鏂囨。鏍″噯鎻愰啋涓庨樆鏂?   - 鏀剁揣渚濊禆娌荤悊锛氬畨鍏?advisory 鍒嗙骇澶勭悊銆佷緥澶栫櫥璁般€佸埌鏈熷鏍?4. 鍙戝竷婕旂粌
   - 瀵?release candidate 鎵ц涓€娆″畬鏁粹€滃彲淇￠獙璇?+ 鐪熷疄鏍锋湰鍥炲綊 + 瀹夊叏鍥炲綊 + 鎬ц兘鍥炲綊鈥?   - 褰㈡垚鍙戝竷璇勫垎鍗°€侀仐鐣欓闄╃櫥璁般€佽眮鍏嶅鎵逛笌鍥為€€璇存槑

#### 闃舵楠屾敹鏍囧噯

- 瀵煎嚭銆丮CP銆佸獟浣?handle銆侀敊璇劚鏁忓叏閮ㄨ繘鍏ヨ嚜鍔ㄥ寲鍥炲綊
- 瀹¤璁板綍鍙鐩栧鍑恒€佽繛鎺ャ€佸け璐ャ€佹嫆缁濄€佽鐩栫‘璁ょ瓑鍏抽敭鍔ㄤ綔
- 鍙戝竷璇勫垎鍗°€侀仐鐣欓闄╂竻鍗曘€佽眮鍏嶆満鍒朵笌鍥為€€娴佺▼瀹屾垚骞跺疄闄呮紨缁冧竴娆?- 鏂囨。婕傜Щ妫€鏌ヤ笌渚濊禆瀹夊叏闂ㄧ鎺ュ叆 release candidate 娴佺▼

#### 褰撳墠浜у搧鍐呰惤鍦拌繘搴︼紙2026-06-13锛?
- 宸叉湁鐪熷疄鍙閾捐矾锛?  - `V2GovernanceSnapshotDto`
  - `supportMatrixEntries`
  - `errorTaxonomyEntries`
  - `releaseGates`
  - `releaseScorecard`
- 褰撳墠 `releaseScorecard` 宸蹭笉鍐嶆槸瀹屽叏闈欐€佸垎鍊硷紝鑰屾槸寮€濮嬬敱锛?  - `releaseGates`
  - `runtimeSignals`
  娲剧敓鍥涗釜缁村害鍒嗘暟
- 褰撳墠 `runtimeSignals` 宸茶繘涓€姝ユ帴鍏ョ湡瀹炲叧鑱斿垎鏋愬揩鐓ф淳鐢熷€硷細
  - `correlationSnapshotAvailable`
  - `correlationLeadCount`
  - `correlationHighConfidenceLeadCount`
  - `correlationReviewLeadCount`
  - `correlationClusterCount`
- 褰撳墠 `runtimeSignals` 杩涗竴姝ユ帴鍏ヨ鍒欏鏃忚鐩栦俊鍙凤細
  - `correlationRuleFamilyCount`
  - `correlationCoveredFamilyCount`
  - `correlationHighConfidenceFamilyCount`
  - `correlationFamilyCoverage[]`
- 褰撳墠 `/v2` 宸插彲鎸?`LNK / Prefetch / Registry / RecycleBin / BrowserDownload / BrowserHistory / Email / JumpList` 瑙傚療瑙勫垯瀹舵棌瑕嗙洊鎯呭喌
- 褰撳墠鍙戝竷闂ㄧ宸插紑濮嬫秷璐硅繖浜涘鏃忚鐩栦俊鍙凤紝鏂板 `correlation-family-coverage` gate
- 褰撳墠 HTML / JSON / CSV 鎶ュ憡瀵煎嚭涔熷紑濮嬪甫鍑哄鏃忚鐩栨不鐞嗘憳瑕侊紝鍙戝竷鏉愭枡涓嶅啀鍙兘渚濊禆浜у搧椤垫埅鍥?- 褰撳墠 `/v2` 宸插彲浠ョ洿鎺ョ湅鍒板叧鑱斿伐浣滄祦鐨勮繍琛屾€佹憳瑕侊紝鑰屼笉鏄彧鑳借繘鍏?`CorrelationWorkspace` 鍚庡啀浜哄伐鍒ゆ柇
- 杩欎粛鐒跺彧鏄涓€闃舵锛氱湡瀹?fixture / benchmark / 瀹夊叏鍥炲綊缁撴灉灏氭湭瀹屽叏鑷姩娉ㄥ叆璇勫垎鍗★紝鍙槸鍏堟妸璇勫垎鐨勬淳鐢熼鏋舵帴閫?
## 4. 娴嬭瘯鐭╅樀

| 缁村害 | 鍦烘櫙 | 閫氳繃鏍囧噯 |
|---|---|---|
| Fixture 鍙俊鎬?| public-small 鏍锋湰鍙叕寮€澶嶇幇 | 姣忎釜鏍锋湰閮芥湁鏉ユ簮璇存槑銆佸搱甯屻€乪xpected JSON銆佹敮鎸佽兘鍔涙爣绛?|
| E01 | 鍗曟涓庡熀纭€澶氭璇诲彇 | 鍏冩暟鎹€佸垎娈垫嫾鎺ャ€佽鍋忕Щ缁撴灉涓庡熀鍑嗕竴鑷达紱涓嶆敮鎸佸満鏅槑纭惤鍦ㄥ凡鐭ラ檺鍒?|
| NTFS | 姝ｅ父銆佸凡鍒犻櫎銆乭idden/system銆乷rphan | 瀛楁涓?expected JSON 瀵归綈锛涙湭淇濊瘉瀛楁琚爣娉紝涓嶉潤榛樹吉閫?|
| Prefetch | 杩愯娆℃暟銆佹椂闂存埑銆佸叧鑱斿彲鎵ц璺緞 | 涓庡熀鍑嗗伐鍏锋垨浜哄伐鏍囨敞涓€鑷达紱涓嶆敮鎸佸帇缂╁彉浣撴槑纭爣娉?|
| LNK | target銆乿olume銆乼imestamps銆乺elative path | 杈撳嚭缁撴瀯绋冲畾锛屽彲鍥炶烦鍒板師濮嬭褰?|
| Registry | key/value/time 鍩虹瑙ｆ瀽 | hive 鍩虹瀛楁绋冲畾锛涗簨鍔℃棩蹇楁湭鏀寔鍦烘櫙鏄庣‘鍒楀叆闄愬埗 |
| Recycle Bin | 鍘熻矾寰勩€佸垹闄ゆ椂闂淬€佹枃浠跺ぇ灏?| 涓庢牱鏈熀鍑嗕竴鑷达紝鍙叧鑱斿埌 deleted file |
| Browser | Chrome / Edge / Firefox smoke | 鍘嗗彶 / 涓嬭浇鏈€灏忓瓧娈电ǔ瀹氾紱鐗堟湰杈圭晫鏄庣‘ |
| Email | EML / EMLX smoke | 涓婚銆佸彂浠朵汉銆佹敹浠朵汉銆佹椂闂淬€侀檮浠跺悕绋冲畾锛汸ST/OST/mbox 鏈壙璇?|
| Expected JSON | 瀛楁婕傜Щ | 闈炲厑璁稿瓧娈靛樊寮傜洿鎺ュけ璐ワ紝鍏佽婕傜Щ瀛楁鏈夌悊鐢辫鏄?|
| 鍏宠仈鍒嗘瀽 | Prefetch / LNK / Registry / Recycle Bin / Browser / Email 璺ㄥ伐浠跺叧鑱?| 姣忔潯 lead 鍙洖婧?provenance锛宑onfidence 瑙勫垯绋冲畾锛岃鍒よ竟鐣屽凡鏂囨。鍖?|
| 鏃堕棿绾夸竴鑷存€?| 鍏宠仈缁撴灉涓?timeline / 鎶ュ憡涓€鑷?| 鐩稿悓璇佹嵁鍦ㄥ悇瑙嗗浘鏃堕棿銆佽矾寰勩€佹潵婧愭弿杩颁竴鑷?|
| 鏂囦欢鏍戜笌鎼滅储 | medium / large case 鎳掑姞杞姐€佸垎椤点€佹帓搴忋€佽繃婊?| 椤哄簭绋冲畾锛屾棤閲嶅/婕忛」锛岀姸鎬佸悗缃鍒欎笉鍥炲綊 |
| 鎬ц兘 | medium case 鐑煡璇?| 鎼滅储 p95 鈮?1.5s锛屾椂闂寸嚎绛涢€?p95 鈮?2s锛屾枃浠舵爲棣栧睍寮€ p95 鈮?800ms |
| 鎬ц兘 | large case 鐑煡璇?| 鎼滅储 p95 鈮?4s锛屾椂闂寸嚎绛涢€?p95 鈮?5s锛屾枃浠舵爲棣栧睍寮€ p95 鈮?2s |
| 鍙栨秷涓庢仮澶?| 闀夸换鍔″彇娑?| UI 500ms 鍐呮敹鍒板彇娑堢‘璁わ紝鍚庣 3s 鍐呭崗浣滃仠姝紝涓嶄骇鐢熻剰鐘舵€?|
| 璧勬簮杈圭晫 | 瀵煎叆 + 鎼滅储 + 鏃堕棿绾块暱鏃惰繍琛?| 鏃犳槑鏄惧彞鏌勬硠婕忥紱宄板€煎唴瀛樹笉瓒呰繃 medium 2.5GB / large 6GB 榛樿闂ㄦ |
| MCP 瀹夊叏 | SSE / stdio 闈炴硶閰嶇疆 | 闈炴硶 URL銆佸祵鍏ュ嚟鎹€乸ath 鍨嬪懡浠ゃ€丯UL 杈撳叆鍏ㄩ儴琚嫆缁濆苟鐣欏璁¤褰?|
| 瀵煎嚭瀹夊叏 | 璺緞绌胯秺銆乷verwrite銆佽法 case 鍙ユ焺 | 榛樿鎷掔粷锛屾樉寮忕‘璁ゆ墠鍙鐩栵紱鍙ユ焺涓嶆硠闇茬湡瀹炶矾寰?|
| 閿欒鑴辨晱 | 瑙ｆ瀽澶辫触銆佺郴缁熼敊璇€佸閮ㄨ繛鎺ュけ璐?| 鍓嶇涓嶅嚭鐜版晱鎰熻矾寰勩€佸嚟鎹€佺幆澧冨彉閲忥紱閿欒鐮佸彲瀹氫綅 |
| 鏂囨。闃叉紓绉?| 鏀寔鐭╅樀銆佸凡鐭ラ檺鍒躲€乥enchmark銆佸畨鍏ㄦā鍨?| 瀹炵幇鍙樻洿鍚庡搴旀枃妗ｅ悓姝ユ洿鏂帮紝鍚﹀垯 gate 澶辫触 |
| 鍙戝竷 | release candidate 鍏ㄥ洖褰?| 鏍稿績 fixture銆佺湡瀹炴牱鏈€佹€ц兘銆佸畨鍏ㄥ洓绫诲洖褰掑叏閮ㄩ€氳繃 |

## 5. 璇勫垎鏈哄埗

鎬诲垎 100 鍒嗭紝闃舵杈炬爣涓庤瘎鍒嗗悓鏃朵娇鐢紱浠讳竴纭棬绂佸け璐ュ垯鎬昏瘎鐩存帴涓嶅悎鏍笺€?
### 5.1 鍒嗗€兼瀯鎴?
- V2-1 鍙俊楠岃瘉浣撶郴锛?0 鍒?- V2-2 澶氬伐浠跺叧鑱斿垎鏋愶細25 鍒?- V2-3 鎬ц兘涓庣ǔ瀹氭€э細20 鍒?- V2-4 瀹夊叏娌荤悊涓庡彂甯冩不鐞嗭細25 鍒?
### 5.2 鍗曢樁娈佃瘎鍒嗚鍒?
- 100%锛氳闃舵鎵€鏈夐獙鏀堕」杈炬垚锛屼笖鏃犳柊澧炴湭鐧昏 P0/P1 椋庨櫓
- 80%锛氭牳蹇冮獙鏀堕」杈炬垚锛屽瓨鍦ㄥ凡鐧昏浣嗗彲鎺ュ彈鐨?P2 椋庨櫓锛屼笖宸叉湁鍥炲綊涓庤ˉ鏁戣鍒?- 60%锛氫富閾捐矾鍙敤锛屼絾浠嶇己鍏抽敭鑷姩鍖栥€佹枃妗ｆ垨鐪熷疄鏍锋湰璇佹槑
- 0%锛氳闃舵鍑虹幇纭棬绂佸け璐?
### 5.3 纭棬绂?
- 鏍稿績閾捐矾 fixture 鍥炲綊澶辫触
- 鏀寔鐭╅樀涓庣湡瀹炲疄鐜颁笉涓€鑷?- 瀵煎嚭 / MCP / 濯掍綋杈圭晫瀛樺湪鍙鐜板畨鍏ㄧ粫杩?- 鐪熷疄鏍锋湰鍥炲綊鏃犳硶璇存槑楠岃瘉鑼冨洿涓庢湭淇濊瘉瀛楁
- 鍙戝竷鏂囨。涓ラ噸婕傜Щ涓旀棤璞佸厤瀹℃壒

### 5.4 鎬昏瘎瑙ｉ噴

- A锛?0-100锛夛細鍙繘鍏?V2 鍙戝竷鏀跺熬
- B锛?0-89锛夛細鍙繘鍏ュ€欓€夊彂甯冿紝浣嗛渶鍏抽棴鍏ㄩ儴 P1
- C锛?0-79锛夛細浠呭彲缁х画鍐呮祴锛屼笉鍙澶栧绉拌兘鍔涚ǔ瀹?- D锛?70锛夛細缁х画寮€鍙戯紝涓嶈繘鍏ュ€欓€夊彂甯?
## 6. Agents 鍒嗗伐涓庡崗浣滄満鍒?
### 6.1 鍥哄畾鍒嗗伐

- Kepler锛歊ust 鍚庣涓昏矗
  - parser銆乫ixture harness銆乪xpected JSON 瀵规瘮銆佸叧鑱旇鍒欏紩鎿庛€乥enchmark銆丮CP / 瀵煎嚭 / 濯掍綋瀹夊叏杈圭晫銆佸璁¤褰?- Poincare锛氬墠绔富璐?  - 鍙俊楠岃瘉闈㈡澘銆佸叧鑱斿垎鏋愬伐浣滃彴銆佹姤鍛婂睍绀恒€佹€ц兘鍙鍖栥€佸畨鍏ㄥ璁￠潰鏉裤€佸叕鏈夌粍浠舵矇娣€銆佸墠绔洖褰掓祴璇?- Gauss锛氭祴璇曚笌鏁版嵁璧勪骇涓昏矗
  - public-small / public-medium / private-real 鏍锋湰娌荤悊銆乪xpected JSON 缁存姢銆佺湡瀹炴牱鏈洖褰掕鏄庛€乥enchmark 鏁版嵁绠＄悊銆佹枃妗ｉ槻婕傜Щ妫€鏌?- Codex 涓荤嚎绋嬶細绯荤粺闆嗘垚涓庡彂甯冧富璐?  - 璺ㄧ濂戠害瀹℃煡銆侀樁娈佃竟鐣屾妸鎺с€丮ermaid / 鏂囨。鏇存柊銆佸彂甯冭瘎鍒嗗崱銆侀闄╃櫥璁般€佹渶缁堥泦鎴愰獙鏀?
### 6.2 鍗忎綔鏈哄埗

- 闃舵椤哄簭浠?`V2-1 鈫?V2-2 鈫?V2-3 鈫?V2-4` 涓轰富绾?- 鍏佽骞惰鐨勯儴鍒嗭細
  - V2-4 鐨勬潈闄愭ā鍨嬩笌瀹¤璁板綍鍙湪 V2-1 Phase 2 鍚庡苟琛屽惎鍔?  - V2-3 benchmark harness 鍙湪 V2-1 fixture 浣撶郴绋冲畾鍚庢彁鍓嶅惎鍔?- 姣忎釜 Phase 榛樿 2 鍛紝姣忎釜 Stage 缁撴潫杩藉姞 1 娆￠泦鎴愬懆
- 姣忓懆鍥哄畾杈撳嚭 4 浠朵簨锛?  - 鍙樻洿鎽樿
  - 椋庨櫓澧為噺
  - 鍥炲綊缁撴灉
  - 鏂囨。鍚屾鐘舵€?
## 7. 鍋囪涓庨粯璁?
- V2 浼樺厛鎶婂綋鍓?Windows 鏍稿績閾捐矾鍋氭繁鍋氱ǔ锛屼笉鎵╁紶鍒板 OS 鍏ㄨ鐩?- Email 榛樿浠嶆槸 EML/EMLX-first锛汸ST/OST/mbox 鍙繘鍏ユ帰绱㈡垨 V3
- Registry 缁х画浠ュ綋鍓嶈В鏋愯兘鍔涙牎鍑嗭紝涓嶆壙璇哄湪 V2 瀹屾垚瀹屾暣 Registry 娴忚鍣ㄤ笌浜嬪姟鏃ュ織閲嶆斁
- FAT/exFAT deleted recovery銆佸鏉傚娈?E01銆佸叏閮?Prefetch 鍘嬬缉鍙樹綋銆佸叏閮ㄦ祻瑙堝櫒鐗堟湰鍏煎鎬э紝浠嶅繀椤绘槑纭啓鍦ㄥ凡鐭ラ檺鍒朵腑
- 鏂囨。銆乫ixture 璇存槑銆乪xpected JSON銆侀敊璇垎绫汇€乥enchmark 璇存槑缁熶竴閲囩敤 UTF-8

## 8. V3 鏂瑰悜

- 寤虹珛 Evidence Graph锛屾妸鏂囦欢銆乤rtifact銆乼imeline銆乪ntity銆乴ead 缁熶竴鎴愬彲鏌ヨ鍥炬ā鍨?- 鎵╁睍瀹瑰櫒涓庣郴缁熻鐩栭潰锛歅ST/OST/mbox銆丷egistry transaction logs銆佹洿澶氭祻瑙堝櫒鐗堟湰銆佹洿澶氭枃浠剁郴缁熶笌 Linux/macOS 宸ヤ欢
- 寮曞叆鍙鐜拌皟鏌ュ彊浜嬶細case notebook銆佽瘉鎹紩鐢ㄣ€佸垎鏋愭楠ゅ鎾€佹姤鍛婁笌鎿嶄綔鍘嗗彶鑱斿姩
- 寤虹珛瑙勫垯鍖呬笌妯℃澘鏈哄埗锛氳皟鏌ユā鏉裤€佸懡涓鍒欏寘銆佺粍缁囩骇楠岃瘉閰嶇疆銆佽В閲婃€ц瘎鍒嗙瓥鐣?- 璇勪及绂荤嚎鎵瑰鐞嗕笌澶氶樁娈靛鍏?orchestration锛氬湪淇濇寔妗岄潰浼樺厛鍓嶆彁涓嬶紝涓鸿秴澶?case 鎻愪緵鍙仮澶嶃€佸彲鎺掗槦銆佸彲鍒嗛樁娈垫墽琛岀殑鏈湴鎵瑰鐞嗚兘鍔?
