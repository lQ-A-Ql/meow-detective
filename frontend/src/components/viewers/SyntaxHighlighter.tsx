/**
 * SyntaxHighlighter - 语法高亮组件
 *
 * 使用 highlight.js 实现代码语法高亮。
 */

import { useMemo } from 'react';
import hljs from 'highlight.js/lib/core';

// 按需加载语言
import javascript from 'highlight.js/lib/languages/javascript';
import typescript from 'highlight.js/lib/languages/typescript';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import go from 'highlight.js/lib/languages/go';
import java from 'highlight.js/lib/languages/java';
import cpp from 'highlight.js/lib/languages/cpp';
import csharp from 'highlight.js/lib/languages/csharp';
import html from 'highlight.js/lib/languages/xml';
import css from 'highlight.js/lib/languages/css';
import json from 'highlight.js/lib/languages/json';
import xml from 'highlight.js/lib/languages/xml';
import yaml from 'highlight.js/lib/languages/yaml';
import sql from 'highlight.js/lib/languages/sql';
import shell from 'highlight.js/lib/languages/shell';
import markdown from 'highlight.js/lib/languages/markdown';
import bash from 'highlight.js/lib/languages/bash';
import dockerfile from 'highlight.js/lib/languages/dockerfile';

// 注册语言
hljs.registerLanguage('javascript', javascript);
hljs.registerLanguage('typescript', typescript);
hljs.registerLanguage('python', python);
hljs.registerLanguage('rust', rust);
hljs.registerLanguage('go', go);
hljs.registerLanguage('java', java);
hljs.registerLanguage('cpp', cpp);
hljs.registerLanguage('csharp', csharp);
hljs.registerLanguage('html', html);
hljs.registerLanguage('css', css);
hljs.registerLanguage('json', json);
hljs.registerLanguage('xml', xml);
hljs.registerLanguage('yaml', yaml);
hljs.registerLanguage('sql', sql);
hljs.registerLanguage('shell', shell);
hljs.registerLanguage('markdown', markdown);
hljs.registerLanguage('bash', bash);
hljs.registerLanguage('dockerfile', dockerfile);

interface SyntaxHighlighterProps {
  /** 代码内容 */
  code: string;
  /** 语言 */
  language?: string;
}

/** 根据扩展名获取语言 */
function getLanguageFromExtension(ext: string): string | undefined {
  const map: Record<string, string> = {
    js: 'javascript',
    jsx: 'javascript',
    mjs: 'javascript',
    ts: 'typescript',
    tsx: 'typescript',
    py: 'python',
    rs: 'rust',
    go: 'go',
    java: 'java',
    c: 'cpp',
    h: 'cpp',
    cpp: 'cpp',
    cc: 'cpp',
    cs: 'csharp',
    html: 'html',
    htm: 'html',
    css: 'css',
    scss: 'css',
    json: 'json',
    xml: 'xml',
    svg: 'xml',
    yaml: 'yaml',
    yml: 'yaml',
    sql: 'sql',
    sh: 'shell',
    bash: 'bash',
    md: 'markdown',
    dockerfile: 'dockerfile',
  };
  return map[ext.toLowerCase()];
}

export function SyntaxHighlighter({
  code,
  language,
}: SyntaxHighlighterProps) {
  // 高亮代码
  const highlightedHtml = useMemo(() => {
    if (!code) return '';

    try {
      if (language) {
        const result = hljs.highlight(code, { language, ignoreIllegals: true });
        return result.value;
      }

      // 自动检测语言
      const result = hljs.highlightAuto(code);
      return result.value;
    } catch {
      // 高亮失败，返回纯文本
      return code
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;');
    }
  }, [code, language]);

  // 分割为行并添加行号
  const lines = useMemo(() => {
    const htmlLines = highlightedHtml.split('\n');
    return htmlLines;
  }, [highlightedHtml]);

  return (
    <div className="font-mono text-[11px] leading-[18px]">
      {lines.map((lineHtml, index) => (
        <div key={index} className="flex hover:bg-[#f8f8f8]">
          {/* 行号 */}
          <div className="w-12 shrink-0 text-right text-[#999] select-none border-r border-[#eee] bg-[#fafafa] px-2">
            {index + 1}
          </div>
          {/* 代码 */}
          <div
            className="flex-1 px-3 whitespace-pre-wrap break-all min-w-0"
            dangerouslySetInnerHTML={{ __html: lineHtml || '\u00A0' }}
          />
        </div>
      ))}
    </div>
  );
}

export { getLanguageFromExtension };
