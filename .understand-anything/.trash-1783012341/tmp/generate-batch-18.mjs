import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { resolve, dirname } from 'path';

// ── Read extraction results and batch data ──
const extractPath = 'D:/process/forensic/.understand-anything/tmp/ua-file-extract-results-18.json';
const inputPath = 'D:/process/forensic/.understand-anything/tmp/ua-file-analyzer-input-18.json';

const extract = JSON.parse(readFileSync(extractPath, 'utf8'));
const batchData = JSON.parse(readFileSync(inputPath, 'utf8'));
const importData = batchData.batchImportData;

// ── File metadata (Chinese summaries and tags) ──
const fileMeta = {
  "crates/evtx-patched/src/binxml/array_expand.rs": {
    summary: "EVTX Binary XML 数组展开模块：检测 XML 元素中的数组值，将数组子节点展开为重复的标量元素，处理属性内数组和子节点数组两种位置。",
    tags: ["evtx-parser", "array-expansion", "binary-xml", "data-transformation"],
    complexity: "complex"
  },
  "crates/evtx-patched/src/binxml/ir.rs": {
    summary: "EVTX Binary XML 中间表示(IR)构建器：将原始二进制 XML 字节流解析为结构化 IR 树，包含模板实例化、名称引用展开、子树构建和解析缓存。",
    tags: ["evtx-parser", "binary-xml", "intermediate-representation", "tree-builder", "template-resolution"],
    complexity: "complex",
    languageNotes: "使用 bump allocator (IrArena) 管理 IR 节点生命周期，支持 Record 和 TemplateDefinition 两种构建模式。"
  },
  "crates/evtx-patched/src/binxml/ir_json.rs": {
    summary: "EVTX IR 转 JSON 序列化器：将二进制 XML 中间表示树渲染为结构化 JSON，支持增量属性分离、重复元素名去重、文本内容和数值的类型推断。",
    tags: ["evtx-parser", "json-serialization", "intermediate-representation", "data-export"],
    complexity: "complex",
    languageNotes: "针对 EVTX 日志的 EventData 结构做特殊处理：当元素有子元素且包含文本时，拆分为 '#text' 和子元素两部分。"
  },
  "crates/evtx-patched/src/binxml/ir_xml.rs": {
    summary: "EVTX IR 转 XML 序列化器：将二进制 XML 中间表示树渲染为人类可读的 XML 文本，支持缩进格式、UTF-16 转义和空属性值检测。",
    tags: ["evtx-parser", "xml-serialization", "intermediate-representation", "data-export"],
    complexity: "complex"
  },
  "crates/evtx-patched/src/binxml/mod.rs": {
    summary: "binxml 模块入口：重导出数组展开、IR 构建、JSON/XML 序列化、名称解析、令牌解析和值渲染等所有子模块。",
    tags: ["barrel", "module-root", "evtx-parser", "binary-xml"],
    complexity: "simple"
  },
  "crates/evtx-patched/src/binxml/name.rs": {
    summary: "BinXml 名称解析模块：从 EVTX 模板定义和字符串缓存中解析二进制 XML 元素名称和属性名称，支持偏移量引用和 Wevt 内联名称编码。",
    tags: ["evtx-parser", "binary-xml", "name-resolution", "string-encoding"],
    complexity: "moderate"
  },
  "crates/evtx-patched/src/binxml/tokens.rs": {
    summary: "BinXml 令牌解析模块：定义 EVTX 二进制 XML 的令牌结构体（开放元素、属性、实体引用、模板值、替换描述符等）及其从字节流反序列化的解析逻辑。",
    tags: ["evtx-parser", "binary-xml", "token-parsing", "deserialization"],
    complexity: "complex",
    languageNotes: "令牌解析采用 Cursor 模式，每个结构体都有对应的 read_*_cursor 函数，统一从 ByteCursor 中读取并校验数据大小。"
  },
  "crates/evtx-patched/src/binxml/value_render.rs": {
    summary: "BinXml 值渲染器：将 EVTX 的二进制值类型（整数、浮点、十六进制、日期时间、SID 等）格式化为人类可读的 JSON/XML 文本，处理转义和分隔。",
    tags: ["evtx-parser", "value-rendering", "string-formatting", "data-display"],
    complexity: "complex"
  },
  "crates/evtx-patched/src/binxml/value_variant.rs": {
    summary: "BinXml 值类型定义与反序列化：定义 EVTX 所有支持的 BinXmlValue 枚举变体（标量和数组类型共 50+ 种），实现从二进制游标反序列化、数组展开和数组项提取。",
    tags: ["evtx-parser", "value-types", "deserialization", "variant-enum", "array-handling"],
    complexity: "complex",
    languageNotes: "EVTX 的 BinXmlValueType 枚举共 50+ 个变体，涵盖 Null、String、各种整数、浮点、GUID、SID、FileTime、SysTime 及其对应的数组类型。"
  },
  "crates/evtx-patched/src/err.rs": {
    summary: "EVTX 解析器错误类型中心：定义所有解析错误的枚举层次（输入错误、反序列化错误、序列化错误、块错误），提供错误上下文捕获（hexdump、偏移量、IO 消息）。",
    tags: ["evtx-parser", "error-handling", "error-types", "diagnostics"],
    complexity: "moderate",
    languageNotes: "采用分层错误架构：EvtxError 为顶层错误，聚合 InputError、SerializationError、DeserializationError、ChunkError 和 IO 错误。"
  },
  "crates/evtx-patched/src/evtx_chunk.rs": {
    summary: "EVTX 块解析与迭代：解析 EVTX 文件的单个 chunk（头部、数据、字符串缓存），提供记录迭代器，支持校验和验证和模板缓存复用。",
    tags: ["evtx-parser", "chunk-parsing", "iteration", "checksum-validation"],
    complexity: "complex"
  },
  "crates/evtx-patched/src/evtx_file_header.rs": {
    summary: "EVTX 文件头解析：读取 EVTX 文件的文件头结构（魔法数、版本、块计数、校验和），验证文件格式完整性。",
    tags: ["evtx-parser", "file-header", "format-validation"],
    complexity: "moderate"
  },
  "crates/evtx-patched/src/evtx_parser.rs": {
    summary: "EVTX 解析器主入口：管理 EVTX 文件的整体解析流程（文件头验证、按块迭代、多线程并行记录序列化），提供 ParserSettings 配置和 ReadSeek trait 抽象。",
    tags: ["evtx-parser", "entry-point", "parser-orchestration", "multi-threading"],
    complexity: "complex",
    languageNotes: "使用 ReadSeek trait 抽象数据源（文件、内存缓冲区），支持多线程并行处理 chunk 并返回 JSON/XML 记录流。"
  },
  "crates/evtx-patched/src/evtx_record.rs": {
    summary: "EVTX 记录解析与序列化：解析单个 EVTX 事件记录（事件 ID、时间戳、二进制 XML 数据），提供 JSON 和 XML 两种输出格式以及模板实例解析。",
    tags: ["evtx-parser", "record-parsing", "event-serialization", "template-instantiation"],
    complexity: "complex"
  },
  "crates/evtx-patched/src/lib.rs": {
    summary: "evtx-patched 包根模块：重导出解析器、记录、块、文件头、错误类型、IR 模型和工具模块，提供 CRC32 IEEE 校验和工具函数。",
    tags: ["evtx-parser", "entry-point", "barrel", "crate-root"],
    complexity: "simple"
  },
  "crates/evtx-patched/src/model/ir.rs": {
    summary: "EVTX IR 数据模型：定义中间表示的核心数据类型（Node、Element、Text、Attrs、Placeholder、TemplateValue 等），基于 bump allocator 的 IrArena 和 IrTree。",
    tags: ["evtx-parser", "data-model", "intermediate-representation", "arena-allocator"],
    complexity: "complex",
    languageNotes: "IrArena 使用 bumpalo::Bump 分配器批量管理节点生命周期；Node 枚举包含 Element/Text/Value/EntityRef 等 9 个变体覆盖所有 BinXml 结构。"
  },
  "crates/evtx-patched/src/model/ir_visit.rs": {
    summary: "EVTX IR 访问者模式：定义 IrVisitor trait 接口（start_element/end_element/visit_text 等回调），提供 walk_ir 树遍历实现。",
    tags: ["evtx-parser", "visitor-pattern", "tree-traversal", "intermediate-representation"],
    complexity: "simple"
  },
  "crates/evtx-patched/src/model/mod.rs": {
    summary: "model 模块入口：重导出 IR 数据模型和 IR 访问者模块。",
    tags: ["barrel", "module-root", "evtx-parser", "data-model"],
    complexity: "simple"
  },
  "crates/evtx-patched/src/string_cache.rs": {
    summary: "EVTX 字符串缓存：在 EVTX chunk 解析时按偏移量/长度索引预计算的字符串表，加速 BinXml 名称查找。",
    tags: ["evtx-parser", "string-cache", "performance", "lookup"],
    complexity: "simple"
  },
  "crates/evtx-patched/src/utils/byte_cursor.rs": {
    summary: "字节游标工具：提供 ByteCursor 结构体封装缓冲区位置追踪和类型化读取（u8/u16/u32/u64、SID、UTF-16 字符串、长度前缀字符串），统一 EVTX 二进制数据的读取模式。",
    tags: ["evtx-parser", "binary-parsing", "cursor", "utility", "low-level"],
    complexity: "complex"
  }
};

// ── Function/class metadata generators ──
function fnTag(fname) {
  if (fname.startsWith('from_') || fname.startsWith('read_') || fname.startsWith('parse_')) return ['parser', 'deserialization'];
  if (fname.startsWith('write_') || fname.startsWith('render_') || fname.startsWith('into_')) return ['serialization', 'rendering'];
  if (fname.startsWith('build_') || fname.startsWith('instantiate_') || fname.startsWith('process_')) return ['builder', 'tree-construction'];
  if (fname.startsWith('expand_') || fname.startsWith('clone_') || fname.startsWith('resolve_')) return ['transformation', 'tree-manipulation'];
  if (fname.startsWith('validate_') || fname.startsWith('check_')) return ['validation', 'integrity'];
  if (fname.startsWith('find_') || fname.startsWith('get_') || fname.startsWith('has_')) return ['query', 'lookup'];
  if (fname.startsWith('bench_') || fname.startsWith('test_')) return ['benchmark', 'testing'];
  if (fname.startsWith('new') || fname.startsWith('with_')) return ['constructor', 'factory'];
  if (fname.startsWith('push_') || fname.startsWith('attach_') || fname.startsWith('finish_')) return ['builder', 'mutation'];
  return ['utility'];
}

function classTag(cname) {
  if (cname.endsWith('Error') || cname.endsWith('Error_')) return ['error-type', 'diagnostics'];
  if (cname.endsWith('Builder') || cname.endsWith('Emitter')) return ['builder', 'serialization'];
  if (cname.endsWith('Visitor') || cname.endsWith('Walker')) return ['visitor-pattern', 'traversal'];
  if (cname.endsWith('Cache') || cname.endsWith('Cache_')) return ['cache', 'performance'];
  if (cname.endsWith('Header') || cname.endsWith('Header_')) return ['header', 'format'];
  if (cname.endsWith('Settings') || cname.endsWith('Config')) return ['configuration', 'settings'];
  if (cname.includes('Node') || cname.includes('Element') || cname.includes('Text') || cname.includes('Attr')) return ['data-model', 'ast-node'];
  if (cname.includes('Value') || cname.includes('Variant')) return ['value-type', 'variant'];
  return ['data-structure', 'type-definition'];
}

function fnSummary(fname, lineCount, fileContext) {
  const map = {
    "expand_array_substitutions_in_element": "递归展开元素中所有数组替换节点，直到没有数组需要展开。",
    "expand_first_array_in_element": "执行元素的第一次数组展开，定位第一个数组值并生成重复元素副本。",
    "clone_element_with_replacement": "克隆元素结构，在指定位置（子节点或属性）用标量替换数组值。",
    "find_first_array_value": "深度优先搜索元素树中第一个可展开的数组值，返回位置信息。",
    "scalar_replacement_from_array_value": "从数组值中提取索引位置的标量替换项，处理字符串数组和通用值数组。",
    "build_tree_from_binxml_bytes_direct": "将原始 BinXml 字节流直接构建为 IR 树，包含模板解析和名称引用展开。",
    "build_tree_from_binxml_bytes_direct_root": "构建 BinXml 字节流的根级 IR 树，内部调用 build_tree_from_binxml_bytes_direct。",
    "build_wevt_template_definition_ir": "解析 Wevt 模板定义的二进制 XML 并构建 IR 树。",
    "instantiate_template_definition_ir": "从已解析的模板定义 IR 树实例化事件记录。",
    "instantiate_template_direct_values": "将模板替换值直接注入元素结构，生成具体的元素实例。",
    "get_or_parse_template_direct": "获取已缓存的模板或从 chunk 模板定义偏移量解析新模板。",
    "process_open_start_element": "处理开放元素令牌，在当前父节点下创建新元素节点。",
    "process_substitution": "处理替换令牌，将模板值或内联替换插入当前位置。",
    "process_value": "处理文本值令牌，根据值类型创建 Text 或 Value 节点。",
    "process_template_instance_values": "处理模板实例值，将模板参数值应用到已解析的元素树。",
    "expand_name_ref": "展开名称引用令牌，将偏移量引用解析为实际的字符串名称。",
    "push_node": "将新节点压入当前上下文（处理文本与值合并、空元素闭合）。",
    "render_json_record": "顶层 JSON 渲染入口：将 IR 树渲染为 serde_json::Value 结构。",
    "write_json_text_content": "写 JSON 文本内容，处理嵌套元素的文本提取和 '#text' 键分离。",
    "write_element_body_json": "递归写元素体为 JSON 对象，处理子元素、文本内容、属性和重复元素名去重。",
    "render_separate_attributes_for_element": "渲染元素的属性对象，支持按 flag 分离 '#attributes' 子对象。",
    "render_xml_record": "顶层 XML 渲染入口：将 IR 树渲染为缩进格式的 XML 字符串。",
    "render_element": "递归渲染单个元素为 XML，处理属性、子节点、空元素简写。",
    "render_nodes": "渲染节点列表为 XML，处理元素间换行和缩进。",
    "from_cursor": "从字节游标中解析 BinXml 名称（偏移量引用方式），读取 data_size 和单字节长度前缀。",
    "from_cursor_wevt_inline": "从字节游标解析 Wevt 内联名称，读取 4 字节哈希、长度和 UTF-16 名称数据。",
    "read_template_values_cursor": "读取模板实例值（TemplateValues）令牌，包含模板 ID、定义偏移量和值数组。",
    "read_open_start_element_cursor": "读取开放起始元素令牌，包含 data_size、依赖 ID、名称和属性列表。",
    "read_substitution_descriptor_cursor": "读取替换描述符令牌，包含替换索引、值类型和可选标志。",
    "write_value_text": "渲染 BinXmlValue 为文本字符串，根据格式模式（JSON/XML）选择转义策略。",
    "write_hex_u32_lower": "写 u32 为小写十六进制字符串到缓冲区（如 '0x12ab'）。",
    "write_datetime": "写 FileTime 值格式化为 ISO 8601 日期时间字符串。",
    "deserialize_value_type_cursor_in": "核心值类型反序列化：从字节游标读取类型标签，根据类型变体读取对应长度的数据。",
    "expandable_array_len": "检查值是否为数组类型并返回其可展开的元素数量。",
    "array_item_as_value": "从数组值中提取指定索引的单个标量值。",
    "from_binxml_cursor_in": "从二进制 XML 游标就地反序列化值类型，支持嵌套类型读取。",
    "capture_hexdump": "捕获偏移量处的十六进制转储，用于错误诊断。",
    "io_error_with_message": "创建带上下文消息和十六进制转储的包装 IO 错误。",
    "serialized_records": "按块迭代解析并将记录序列化为 JSON，支持多线程。",
    "from_read_seek": "从实现 Read + Seek 的数据源创建 EvtxParser 实例。",
    "from_path": "从文件路径创建 EvtxParser 实例。",
    "find_next_chunk": "定位下一个 EVTX chunk 的起始位置。",
    "allocate_chunk": "分配并读取下一个 EVTX chunk 的数据缓冲区。",
    "template_instances": "解析记录的模板实例引用，获取模板定义中替换项与值的映射。",
    "into_json_value": "将 EVTX 记录转换为 serde_json::Value。",
    "into_json": "将 EVTX 记录序列化为 JSON 字符串。",
    "into_xml": "将 EVTX 记录序列化为 XML 字符串。",
    "from_bytes_at": "从指定偏移量解析 EVTX 记录头（事件 ID、时间戳、数据大小）。",
    "record_data_size": "计算 EVTX 记录数据段的大小。",
    "validate_data_checksum": "验证 EVTX chunk 的数据校验和。",
    "validate_header_checksum": "验证 EVTX chunk 的头校验和。",
    "new_with_arena": "使用共享 Arena 创建 EVTX chunk 实例。",
    "walk_ir_node": "递归遍历 IR 树的单个节点，按节点类型调用对应的 visitor 回调。",
    "is_optional_empty": "检查可选占位符值是否为空（用于过滤未填充的可选模板值）。",
    "populate": "从二进制字符串表数据填充字符串缓存。",
    "read_sid_ref": "读取安全标识符(SID)引用结构。",
    "read_sized_slice_aligned_in": "读取大小前缀的切片并确保对齐。",
    "utf16_by_char_count": "按字符数读取 UTF-16 编码字符串。",
    "null_terminated_utf16_string": "读取空字符终止的 UTF-16 字符串。",
    "len_prefixed_utf16_string": "读取长度前缀的 UTF-16 字符串。",
    "len_prefixed_utf16_string_utf8": "读取长度前缀的 UTF-16 字符串并转换为 UTF-8。",
    "len_prefixed_utf16_string_bump": "读取长度前缀的 UTF-16 字符串存入 bump allocator。",
    "write_utf16_escaped": "写 UTF-16 字符串并转义 XML/JSON 特殊字符。",
    "write_xml_escaped_str": "写 XML 转义后的字符串。",
    "write_delimited": "写分隔列表（如逗号分隔的十六进制值列表）。",
    "write_float_list": "写浮点数列表。",
    "write_datetime_list": "写日期时间列表。",
    "write_hex_list_u32": "写 u32 十六进制值列表。",
    "clone_and_resolve": "克隆 IR 子树并解析占位符，支持模板定义的深拷贝。",
    "resolve_node_into": "将单个节点递归解析到输出，处理占位符的模板值替换。",
    "finish": "完成树构建，返回根 IR 树。",
    "template_values_from_values": "从事件记录的模板值列表构建 TemplateValue 映射。",
    "ensure_env_logger_initialized": "确保 env_logger 已初始化，用于测试和调试输出。",
  };
  if (map[fname]) return map[fname];
  // Generic fallback
  if (fname.startsWith('new') || fname.startsWith('with_')) return `构造${fileContext || '对象'}的${fname}方法。`;
  if (fname.startsWith('from_')) return `从数据源${fname.slice(5)}解析。`;
  if (fname.startsWith('write_')) return `写入${fname.slice(6).replace(/_/g, ' ')}输出。`;
  if (fname.startsWith('render_')) return `渲染${fname.slice(7).replace(/_/g, ' ')}输出。`;
  if (fname.startsWith('process_')) return `处理${fname.slice(8).replace(/_/g, ' ')}令牌。`;
  if (fname.startsWith('read_')) return `读取${fname.slice(5).replace(/_/g, ' ')}数据。`;
  if (fname.startsWith('push_')) return `推入${fname.slice(5).replace(/_/g, ' ')}到容器。`;
  if (fname.startsWith('clone_')) return `克隆${fname.slice(6).replace(/_/g, ' ')}结构。`;
  if (fname.startsWith('expand_')) return `展开${fname.slice(7).replace(/_/g, ' ')}令牌。`;
  if (fname.startsWith('resolve_')) return `解析${fname.slice(8).replace(/_/g, ' ')}引用。`;
  if (fname.startsWith('build_')) return `构建${fname.slice(6).replace(/_/g, ' ')}结构。`;
  if (fname.startsWith('validate_')) return `验证${fname.slice(9).replace(/_/g, ' ')}完整性。`;
  if (fname.startsWith('find_')) return `查找${fname.slice(5).replace(/_/g, ' ')}。`;
  if (fname.startsWith('get_')) return `获取${fname.slice(4).replace(/_/g, ' ')}。`;
  if (fname.startsWith('instantiate_')) return `实例化${fname.slice(12).replace(/_/g, ' ')}。`;
  if (fname.startsWith('finish')) return `完成${fname.slice(6).replace(/_/g, ' ')}操作。`;
  return `执行${fname.replace(/_/g, ' ')}操作。`;
}

function classSummary(cname, fileContext) {
  const map = {
    "BinXmlValue": "EVTX 二进制 XML 值类型的完整枚举，包含 50+ 种标量和数组类型变体。",
    "BinXmlValueType": "EVTX 值类型的标签枚举，用于反序列化时区分不同值类型的数据大小。",
    "EvtxError": "EVTX 解析器顶层错误枚举，聚合输入、序列化、反序列化、块解析和 IO 五类错误。",
    "DeserializationError": "反序列化错误枚举，覆盖令牌无效、值变体无效、截断、编码错误等场景。",
    "SerializationError": "序列化错误枚举，覆盖 XML/JSON 输出错误和 UTF-8 无效字符。",
    "EvtxParser": "EVTX 文件解析器主体结构，持有文件数据、文件头和解析配置。",
    "ParserSettings": "解析器配置结构，控制线程数、校验和验证、JSON 属性分离、缩进、编码和缓存。",
    "EvtxChunk": "EVTX 块结构，包含块数据、头部、字符串缓存、IR 模板缓存和共享 Arena。",
    "EvtxChunkHeader": "EVTX 块头部结构，包含事件记录区间、数据偏移量、校验和和字符串/模板偏移量表。",
    "EvtxFileHeader": "EVTX 文件头部结构，包含文件格式版本、块计数、下一条记录 ID 和校验和。",
    "EvtxRecord": "EVTX 事件记录结构，包含事件 ID、时间戳、IR 树引用和解析设置。",
    "EvtxRecordHeader": "EVTX 记录头部结构，包含数据大小、事件记录 ID 和时间戳。",
    "Element": "IR 元素节点，包含名称、属性列表、子节点列表和标记字段。",
    "Node": "IR 节点枚举，包含 Element、Text、Value、EntityRef、CharRef、CData、PIData、PITarget、Placeholder 九个变体。",
    "Text": "IR 文本节点，支持 UTF-16 和 UTF-8 两种编码存储。",
    "IrArena": "基于 bumpalo::Bump 的 IR 节点分配器，批量管理所有节点的生命周期。",
    "IrTree": "IR 树根结构，持有 arena 和根节点 ID。",
    "Placeholder": "模板占位符节点，包含替换 ID、值类型和可选标志。",
    "TemplateValue": "模板值枚举，区分值类型和二进制 XML 元素类型两种模板替换项。",
    "IrVisitor": "IR 树访问者 trait，定义 start_element/end_element/visit_text/visit_value 等回调接口。",
    "IrTemplateCache": "IR 模板缓存，存储已解析的模板定义 IR 树以实现跨记录复用。",
    "TreeBuilder": "树构建器状态机，管理标签栈、当前元素、Arena 和编码设置。",
    "ElementBuilder": "元素构建器，管理当前正在构建的元素的名称、属性和值。",
    "JsonEmitter": "JSON 发射器，持有 serde_json 写入器和格式化配置。",
    "XmlEmitter": "XML 发射器，持有写入器、缩进状态、Arena 引用和临时缓冲区。",
    "ValueRenderer": "值渲染器，将 BinXmlValue 类型格式化为人可读的字符串（支持 JSON 和 XML 两种格式）。",
    "ByteCursor": "字节游标工具，封装缓冲区引用和位置索引，提供类型化二进制读取方法。",
    "StringCache": "EVTX 字符串缓存，按偏移量/长度索引字符串表以加速名称查找。",
    "EvtxChunkData": "EVTX 块数据，包含块头部和原始数据字节。",
    "IterChunkRecords": "块记录迭代器，按顺序遍历 EVTX chunk 中的每条事件记录。",
    "IterChunks": "块迭代器，按序迭代 EVTX 文件的所有 chunk。",
    "IntoIterChunks": "消耗性块迭代器，消费解析器并产生 chunk。",
    "BinXmlTemplateValues": "模板实例值结构，包含模板 ID、定义偏移量和替换值列表。",
    "TemplateSubstitutionDescriptor": "模板替换描述符，包含替换索引、值类型和可选/忽略标志。",
    "BinXmlNameEncoding": "名称编码枚举，区分偏移量引用和 Wevt 内联名称两种编码方式。",
    "BinXmlName": "解析后的 BinXml 名称，包装一个字符串切片。",
  };
  if (map[cname]) return map[cname];
  return `${cname} 数据结构。`;
}

// ── Build nodes and edges ──
let nodes = [];
let edges = [];

extract.results.forEach(r => {
  const p = r.path;
  const meta = fileMeta[p] || { summary: '', tags: ['evtx-parser'], complexity: 'moderate' };
  const exportedNames = r.exports ? r.exports.map(e => e.name) : [];
  const complexity = r.nonEmptyLines >= 200 ? 'complex' : (r.nonEmptyLines >= 50 ? 'moderate' : 'simple');

  // ── File node ──
  nodes.push({
    id: `file:${p}`,
    type: 'file',
    name: p.split('/').pop(),
    filePath: p,
    summary: meta.summary,
    tags: meta.tags,
    complexity: complexity
  });

  // ── Import edges ──
  const imports = importData[p] || [];
  imports.forEach(target => {
    edges.push({
      source: `file:${p}`,
      target: `file:${target}`,
      type: 'imports',
      direction: 'forward',
      weight: 0.7
    });
  });

  // ── Function nodes ──
  if (r.functions) {
    const fnNameCount = new Map();
    r.functions.forEach(f => {
      const lineCount = f.endLine - f.startLine + 1;
      if (lineCount >= 10 || exportedNames.includes(f.name)) {
        // Disambiguate duplicate function names within the same file
        const count = fnNameCount.get(f.name) || 0;
        fnNameCount.set(f.name, count + 1);
        const disamb = count > 0 ? `_${count + 1}` : '';
        const fnId = `function:${p}:${f.name}${disamb}`;
        nodes.push({
          id: fnId,
          type: 'function',
          name: f.name,
          filePath: p,
          lineRange: [f.startLine, f.endLine],
          summary: fnSummary(f.name, lineCount, r.path.split('/').pop().replace('.rs','')),
          tags: fnTag(f.name),
          complexity: lineCount >= 50 ? 'complex' : (lineCount >= 20 ? 'moderate' : 'simple')
        });
        edges.push({
          source: `file:${p}`,
          target: fnId,
          type: 'contains',
          direction: 'forward',
          weight: 1.0
        });
        if (exportedNames.includes(f.name)) {
          edges.push({
            source: `file:${p}`,
            target: fnId,
            type: 'exports',
            direction: 'forward',
            weight: 0.8
          });
        }
      }
    });
  }

  // ── Class nodes ──
  if (r.classes) {
    r.classes.forEach(c => {
      const methodCount = c.methods ? c.methods.length : 0;
      const propCount = c.properties ? c.properties.length : 0;
      if (methodCount >= 2 || propCount >= 5 || exportedNames.includes(c.name) || r.path.includes('model/')) {
        nodes.push({
          id: `class:${p}:${c.name}`,
          type: 'class',
          name: c.name,
          filePath: p,
          lineRange: [c.startLine, c.endLine],
          summary: classSummary(c.name, r.path),
          tags: classTag(c.name),
          complexity: methodCount >= 5 ? 'moderate' : 'simple'
        });
        edges.push({
          source: `file:${p}`,
          target: `class:${p}:${c.name}`,
          type: 'contains',
          direction: 'forward',
          weight: 1.0
        });
        if (exportedNames.includes(c.name)) {
          edges.push({
            source: `file:${p}`,
            target: `class:${p}:${c.name}`,
            type: 'exports',
            direction: 'forward',
            weight: 0.8
          });
        }
      }
    });
  }
});

// ── Add cross-batch edges (from neighborMap) ──
// Most cross-batch imports already handled by imports edges.
// Calls edges for known cross-batch functions from neighborMap.
// ir.rs calls render_temp_to_xml (from wevt_templates/render.rs)
// ir_xml.rs calls render_temp_to_xml
// evtx_record.rs calls filetime_to_timestamp (from utils/windows.rs)
// evtx_record.rs calls render_temp_to_xml (from wevt_templates/render.rs)
// err.rs uses read_array, read_u8 etc from utils/bytes.rs

// These are automatically covered by the imports edges already. We don't need explicit
// calls edges for cross-batch since the imports edges cover the dependency.

// ── Reports ──
console.log(`Total nodes: ${nodes.length}`);
console.log(`Total edges: ${edges.length}`);
const importEdges = edges.filter(e => e.type === 'imports').length;
console.log(`Import edges: ${importEdges}`);

// ── Partitioning ──
const maxNodesPerPart = 60;
const maxEdgesPerPart = 120;
const nodeCount = nodes.length;
const edgeCount = edges.length;

// Sort files alphabetically
const files = [...new Set(nodes.filter(n => n.filePath).map(n => n.filePath))].sort();

// Calculate minimum parts needed; add buffer for large files
let parts = Math.ceil(Math.max(nodeCount / maxNodesPerPart, edgeCount / maxEdgesPerPart));
// Ensure each part covers at most 3 files to prevent oversize parts from large files
const minParts = Math.ceil(files.length / 3);
if (parts < minParts) parts = minParts;

const chunkSize = Math.ceil(files.length / parts);

console.log(`Parts: ${parts}, Files per part: ~${chunkSize}`);

const outDir = 'D:/process/forensic/.understand-anything/intermediate';

for (let k = 0; k < parts; k++) {
  const partFiles = files.slice(k * chunkSize, (k + 1) * chunkSize);
  const partFileSet = new Set(partFiles);

  // Nodes belonging to this part
  const partNodes = nodes.filter(n => {
    if (!n.filePath) return false; // shouldn't happen, but safety
    return partFileSet.has(n.filePath);
  });

  // Edges whose source node belongs to this part
  const partEdges = edges.filter(e => {
    const srcNode = nodes.find(n => n.id === e.source);
    if (!srcNode || !srcNode.filePath) return false;
    return partFileSet.has(srcNode.filePath);
  });

  const outFile = parts > 1
    ? `${outDir}/batch-18-part-${k + 1}.json`
    : `${outDir}/batch-18.json`;

  writeFileSync(outFile, JSON.stringify({ nodes: partNodes, edges: partEdges }, null, 2));
  console.log(`  Part ${k + 1}: ${partNodes.length} nodes, ${partEdges.length} edges -> ${outFile}`);
}

console.log('Done!');
