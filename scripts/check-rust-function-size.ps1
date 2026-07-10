# Requires -Version 5.1
<#
.SYNOPSIS
  CI guard: lock production Rust function-size debt during the backend refactor.
.DESCRIPTION
  Scans non-vendored workspace production Rust source with a comment- and
  literal-aware lexer. Functions above the 100-line target must match the exact
  migration baseline and may only shrink, including existing functions above
  150 lines. Any new non-baselined function above 100 lines fails, with 150 as
  the new-code hard ceiling. A reference-revision transition check prevents a
  baseline edit from authorizing itself. Use -GenerateBaseline to print the
  current CSV; the script never writes baselines itself. Use -SelfTest to run
  the in-memory synthetic scanner fixture.
#>
param(
  [string]$BaselinePath,
  [string]$BootstrapManifestPath,
  [string]$TrustedBootstrapSha256,
  [string]$ReferenceRevision,
  [switch]$GenerateBaseline,
  [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
. (Join-Path $PSScriptRoot 'lib/RustGuard.Common.ps1')
if ([string]::IsNullOrWhiteSpace($BaselinePath)) {
  $BaselinePath = Join-Path $repoRoot 'scripts/baselines/rust-function-size-baseline.csv'
}
if ([string]::IsNullOrWhiteSpace($BootstrapManifestPath)) {
  $BootstrapManifestPath = Join-Path $repoRoot 'scripts/baselines/rust-function-size-bootstrap.csv'
}
if ([string]::IsNullOrWhiteSpace($TrustedBootstrapSha256)) {
  $TrustedBootstrapSha256 = $env:RUST_FUNCTION_SIZE_BOOTSTRAP_SHA256
}

$TargetLines = 100
$HardLines = 150
$InitialBootstrapReference = '2087df1cc5209fa879cdb3796e9a1437196bc2f4'
$StrictUtf8 = New-Object System.Text.UTF8Encoding($false, $true)

function Read-StrictUtf8Text {
  param([Parameter(Mandatory = $true)][string]$Path)

  try {
    return $StrictUtf8.GetString([System.IO.File]::ReadAllBytes($Path))
  } catch {
    throw "File is not valid UTF-8: $Path"
  }
}

function Get-NormalizedRelativePath {
  param([Parameter(Mandatory = $true)][string]$FullName)

  return Get-RustGuardRepositoryRelativePath -RepoRoot $repoRoot -FullName $FullName
}

function Get-ProductionRustFiles {
  $files = @()
  foreach ($entry in @(Get-RustGuardFiles -RepoRoot $repoRoot -Mode Production)) {
    $files += $entry.File
  }
  return $files
}

if (-not ('Stage0.RustFunctionScanner' -as [type])) {
  Add-Type -Language CSharp -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Security.Cryptography;
using System.Text;

namespace Stage0
{
    public sealed class RustFunctionRecord
    {
        public string Name { get; set; }
        public string SignatureHash { get; set; }
        public int Occurrence { get; set; }
        public int StartLine { get; set; }
        public int EndLine { get; set; }
        public int Lines { get; set; }
    }

    internal enum RustTokenKind
    {
        Identifier,
        Literal,
        Punctuation
    }

    internal sealed class RustToken
    {
        internal RustToken(RustTokenKind kind, string text, int index, int line)
        {
            Kind = kind;
            Text = text;
            Index = index;
            Line = line;
        }

        internal RustTokenKind Kind;
        internal string Text;
        internal int Index;
        internal int Line;
    }

    internal sealed class TokenRange
    {
        internal TokenRange(int start, int end)
        {
            Start = start;
            End = end;
        }

        internal int Start;
        internal int End;
    }

    public static class RustFunctionScanner
    {
        public static RustFunctionRecord[] Scan(string source)
        {
            if (source == null)
            {
                throw new ArgumentNullException("source");
            }

            List<RustToken> tokens = Tokenize(source);
            int[] matching = BuildDelimiterMap(tokens);
            List<TokenRange> excluded = FindCfgTestItemRanges(tokens, matching);
            List<RustFunctionRecord> records = new List<RustFunctionRecord>();

            int excludedIndex = 0;
            for (int i = 0; i < tokens.Count; i++)
            {
                RustToken token = tokens[i];
                if (token.Kind != RustTokenKind.Identifier || token.Text != "fn")
                {
                    continue;
                }

                while (excludedIndex < excluded.Count && excluded[excludedIndex].End < i)
                {
                    excludedIndex++;
                }
                if (excludedIndex < excluded.Count &&
                    excluded[excludedIndex].Start <= i && i <= excluded[excludedIndex].End)
                {
                    continue;
                }

                int nameIndex = i + 1;
                if (nameIndex >= tokens.Count || tokens[nameIndex].Kind != RustTokenKind.Identifier)
                {
                    continue;
                }

                int endToken;
                int signatureEnd;
                if (!TryFindFunctionEnd(tokens, matching, nameIndex + 1, out signatureEnd, out endToken))
                {
                    continue;
                }

                RustFunctionRecord record = new RustFunctionRecord();
                int declarationStart = FindDeclarationStart(tokens, matching, i);
                record.Name = NormalizeIdentifier(tokens[nameIndex].Text);
                record.SignatureHash = HashSignature(tokens, declarationStart, signatureEnd);
                record.StartLine = tokens[declarationStart].Line;
                record.EndLine = tokens[endToken].Line;
                record.Lines = record.EndLine - record.StartLine + 1;
                records.Add(record);
            }

            records.Sort(delegate(RustFunctionRecord left, RustFunctionRecord right)
            {
                int lineCompare = left.StartLine.CompareTo(right.StartLine);
                if (lineCompare != 0)
                {
                    return lineCompare;
                }
                return StringComparer.Ordinal.Compare(left.Name, right.Name);
            });

            Dictionary<string, int> occurrences = new Dictionary<string, int>(StringComparer.Ordinal);
            foreach (RustFunctionRecord record in records)
            {
                string key = record.Name + "\u001f" + record.SignatureHash;
                int occurrence;
                if (!occurrences.TryGetValue(key, out occurrence))
                {
                    occurrence = 0;
                }
                occurrence++;
                occurrences[key] = occurrence;
                record.Occurrence = occurrence;
            }

            return records.ToArray();
        }

        private static List<RustToken> Tokenize(string source)
        {
            List<RustToken> tokens = new List<RustToken>(Math.Max(64, source.Length / 8));
            int i = 0;
            int line = 1;
            while (i < source.Length)
            {
                char current = source[i];
                if (Char.IsWhiteSpace(current))
                {
                    if (current == '\n')
                    {
                        line++;
                    }
                    i++;
                    continue;
                }

                if (current == '/' && i + 1 < source.Length && source[i + 1] == '/')
                {
                    i += 2;
                    while (i < source.Length && source[i] != '\n')
                    {
                        i++;
                    }
                    continue;
                }

                if (current == '/' && i + 1 < source.Length && source[i + 1] == '*')
                {
                    SkipBlockComment(source, ref i, ref line);
                    continue;
                }

                int literalStart = i;
                int literalLine = line;
                if (TrySkipRawString(source, ref i, ref line))
                {
                    tokens.Add(new RustToken(
                        RustTokenKind.Literal,
                        HashLiteral(source, literalStart, i - literalStart),
                        literalStart,
                        literalLine));
                    continue;
                }

                if ((current == 'b' || current == 'c') &&
                    i + 1 < source.Length && source[i + 1] == '"' &&
                    IsIdentifierBoundary(source, i))
                {
                    literalStart = i;
                    literalLine = line;
                    i++;
                    SkipQuotedString(source, ref i, ref line);
                    tokens.Add(new RustToken(
                        RustTokenKind.Literal,
                        HashLiteral(source, literalStart, i - literalStart),
                        literalStart,
                        literalLine));
                    continue;
                }

                if (current == '"')
                {
                    literalStart = i;
                    literalLine = line;
                    SkipQuotedString(source, ref i, ref line);
                    tokens.Add(new RustToken(
                        RustTokenKind.Literal,
                        HashLiteral(source, literalStart, i - literalStart),
                        literalStart,
                        literalLine));
                    continue;
                }

                if (current == '\'')
                {
                    literalStart = i;
                    literalLine = line;
                    if (TrySkipCharacterLiteral(source, ref i))
                    {
                        tokens.Add(new RustToken(
                            RustTokenKind.Literal,
                            HashLiteral(source, literalStart, i - literalStart),
                            literalStart,
                            literalLine));
                        continue;
                    }
                }

                if (IsIdentifierStart(current))
                {
                    int start = i;
                    i++;
                    while (i < source.Length && IsIdentifierContinue(source[i]))
                    {
                        i++;
                    }
                    if (i == start + 1 && source[start] == 'r' &&
                        i + 1 < source.Length && source[i] == '#' &&
                        IsIdentifierStart(source[i + 1]))
                    {
                        i += 2;
                        while (i < source.Length && IsIdentifierContinue(source[i]))
                        {
                            i++;
                        }
                    }
                    tokens.Add(new RustToken(
                        RustTokenKind.Identifier,
                        source.Substring(start, i - start),
                        start,
                        line));
                    continue;
                }

                tokens.Add(new RustToken(
                    RustTokenKind.Punctuation,
                    current.ToString(),
                    i,
                    line));
                i++;
            }

            return tokens;
        }

        private static void SkipBlockComment(string source, ref int i, ref int line)
        {
            int depth = 1;
            i += 2;
            while (i < source.Length && depth > 0)
            {
                if (source[i] == '\n')
                {
                    line++;
                    i++;
                }
                else if (i + 1 < source.Length && source[i] == '/' && source[i + 1] == '*')
                {
                    depth++;
                    i += 2;
                }
                else if (i + 1 < source.Length && source[i] == '*' && source[i + 1] == '/')
                {
                    depth--;
                    i += 2;
                }
                else
                {
                    i++;
                }
            }
        }

        private static bool TrySkipRawString(string source, ref int i, ref int line)
        {
            int start = i;
            if (!IsIdentifierBoundary(source, start))
            {
                return false;
            }

            int cursor;
            if (source[start] == 'r')
            {
                cursor = start + 1;
            }
            else if ((source[start] == 'b' || source[start] == 'c') &&
                     start + 1 < source.Length && source[start + 1] == 'r')
            {
                cursor = start + 2;
            }
            else
            {
                return false;
            }

            int hashes = 0;
            while (cursor < source.Length && source[cursor] == '#')
            {
                hashes++;
                cursor++;
            }
            if (cursor >= source.Length || source[cursor] != '"')
            {
                return false;
            }

            cursor++;
            while (cursor < source.Length)
            {
                if (source[cursor] == '\n')
                {
                    line++;
                    cursor++;
                    continue;
                }
                if (source[cursor] != '"')
                {
                    cursor++;
                    continue;
                }

                int suffix = cursor + 1;
                int seen = 0;
                while (seen < hashes && suffix < source.Length && source[suffix] == '#')
                {
                    seen++;
                    suffix++;
                }
                if (seen == hashes)
                {
                    i = suffix;
                    return true;
                }
                cursor++;
            }

            i = source.Length;
            return true;
        }

        private static void SkipQuotedString(string source, ref int i, ref int line)
        {
            i++;
            bool escaped = false;
            while (i < source.Length)
            {
                char current = source[i];
                if (current == '\n')
                {
                    line++;
                }
                if (!escaped && current == '"')
                {
                    i++;
                    return;
                }
                if (!escaped && current == '\\')
                {
                    escaped = true;
                }
                else
                {
                    escaped = false;
                }
                i++;
            }
        }

        private static bool TrySkipCharacterLiteral(string source, ref int i)
        {
            int cursor = i + 1;
            if (cursor >= source.Length || source[cursor] == '\n' || source[cursor] == '\r')
            {
                return false;
            }

            if (source[cursor] == '\\')
            {
                cursor++;
                if (cursor >= source.Length)
                {
                    return false;
                }
                if (source[cursor] == 'u' && cursor + 1 < source.Length && source[cursor + 1] == '{')
                {
                    cursor += 2;
                    while (cursor < source.Length && source[cursor] != '}' &&
                           source[cursor] != '\n' && source[cursor] != '\r')
                    {
                        cursor++;
                    }
                    if (cursor >= source.Length || source[cursor] != '}')
                    {
                        return false;
                    }
                    cursor++;
                }
                else if (source[cursor] == 'x')
                {
                    cursor += 3;
                }
                else
                {
                    cursor++;
                }
            }
            else
            {
                if (Char.IsHighSurrogate(source[cursor]) &&
                    cursor + 1 < source.Length && Char.IsLowSurrogate(source[cursor + 1]))
                {
                    cursor += 2;
                }
                else
                {
                    cursor++;
                }
            }

            if (cursor < source.Length && source[cursor] == '\'')
            {
                i = cursor + 1;
                return true;
            }
            return false;
        }

        private static bool IsIdentifierBoundary(string source, int index)
        {
            return index == 0 || !IsIdentifierContinue(source[index - 1]);
        }

        private static bool IsIdentifierStart(char value)
        {
            return value == '_' || Char.IsLetter(value);
        }

        private static bool IsIdentifierContinue(char value)
        {
            return value == '_' || Char.IsLetterOrDigit(value);
        }

        private static int[] BuildDelimiterMap(List<RustToken> tokens)
        {
            int[] matching = new int[tokens.Count];
            for (int i = 0; i < matching.Length; i++)
            {
                matching[i] = -1;
            }

            Stack<int> stack = new Stack<int>();
            for (int i = 0; i < tokens.Count; i++)
            {
                string text = tokens[i].Text;
                if (text == "{" || text == "[" || text == "(")
                {
                    stack.Push(i);
                    continue;
                }
                if (text != "}" && text != "]" && text != ")")
                {
                    continue;
                }
                if (stack.Count == 0)
                {
                    continue;
                }

                int open = stack.Peek();
                if (IsMatchingDelimiter(tokens[open].Text, text))
                {
                    stack.Pop();
                    matching[open] = i;
                    matching[i] = open;
                }
            }
            return matching;
        }

        private static bool IsMatchingDelimiter(string open, string close)
        {
            return (open == "{" && close == "}") ||
                   (open == "[" && close == "]") ||
                   (open == "(" && close == ")");
        }

        private static List<TokenRange> FindCfgTestItemRanges(List<RustToken> tokens, int[] matching)
        {
            List<TokenRange> ranges = new List<TokenRange>();
            for (int i = 0; i + 5 < tokens.Count; i++)
            {
                if (tokens[i].Text != "#" || tokens[i + 1].Text != "[")
                {
                    continue;
                }
                int closeBracket = matching[i + 1];
                if (closeBracket < 0 || !IsProvablyTestOnlyCfgAttribute(tokens, matching, i + 2, closeBracket))
                {
                    continue;
                }

                int itemStart = closeBracket + 1;
                while (itemStart + 1 < tokens.Count &&
                       tokens[itemStart].Text == "#" && tokens[itemStart + 1].Text == "[")
                {
                    int nextClose = matching[itemStart + 1];
                    if (nextClose < 0)
                    {
                        break;
                    }
                    itemStart = nextClose + 1;
                }

                int itemEnd = FindAttributedItemEnd(tokens, matching, itemStart);
                if (itemEnd >= itemStart)
                {
                    ranges.Add(new TokenRange(i, itemEnd));
                    i = closeBracket;
                }
            }

            ranges.Sort(delegate(TokenRange left, TokenRange right)
            {
                return left.Start.CompareTo(right.Start);
            });
            return MergeRanges(ranges);
        }

        private static bool IsProvablyTestOnlyCfgAttribute(
            List<RustToken> tokens,
            int[] matching,
            int start,
            int closeBracket)
        {
            if (start + 3 >= closeBracket || tokens[start].Text != "cfg" || tokens[start + 1].Text != "(")
            {
                return false;
            }
            int closeParen = matching[start + 1];
            if (closeParen != closeBracket - 1)
            {
                return false;
            }

            int cursor = start + 2;
            bool impliesTest;
            return TryParseCfgExpression(tokens, matching, closeParen, ref cursor, out impliesTest) &&
                   cursor == closeParen && impliesTest;
        }

        private static bool TryParseCfgExpression(
            List<RustToken> tokens,
            int[] matching,
            int end,
            ref int cursor,
            out bool impliesTest)
        {
            impliesTest = false;
            if (cursor >= end || tokens[cursor].Kind != RustTokenKind.Identifier)
            {
                return false;
            }

            string predicate = tokens[cursor].Text;
            cursor++;
            if (cursor < end && tokens[cursor].Text == "=")
            {
                cursor++;
                if (cursor >= end)
                {
                    return false;
                }
                cursor++;
                return true;
            }

            if (cursor >= end || tokens[cursor].Text != "(")
            {
                impliesTest = predicate == "test";
                return true;
            }

            int openParen = cursor;
            int closeParen = matching[openParen];
            if (closeParen < 0 || closeParen >= end)
            {
                return false;
            }
            if (predicate != "all" && predicate != "any" && predicate != "not")
            {
                cursor = closeParen + 1;
                return true;
            }

            cursor++;
            List<bool> childImplications = new List<bool>();
            while (cursor < closeParen)
            {
                if (tokens[cursor].Text == ",")
                {
                    cursor++;
                    continue;
                }

                bool childImpliesTest;
                if (!TryParseCfgExpression(tokens, matching, closeParen, ref cursor, out childImpliesTest))
                {
                    return false;
                }
                childImplications.Add(childImpliesTest);
                if (cursor < closeParen && tokens[cursor].Text != ",")
                {
                    return false;
                }
            }
            cursor = closeParen + 1;

            if (predicate == "all")
            {
                foreach (bool child in childImplications)
                {
                    if (child)
                    {
                        impliesTest = true;
                        break;
                    }
                }
            }
            else if (predicate == "any" && childImplications.Count > 0)
            {
                impliesTest = true;
                foreach (bool child in childImplications)
                {
                    if (!child)
                    {
                        impliesTest = false;
                        break;
                    }
                }
            }
            else
            {
                // Negation is deliberately conservative: not(test) is not
                // test-only, and complex negations are retained for scanning.
                impliesTest = false;
            }
            return true;
        }

        private static int FindAttributedItemEnd(List<RustToken> tokens, int[] matching, int start)
        {
            bool semicolonTerminated = IsSemicolonTerminatedAttributedItem(tokens, matching, start);
            for (int i = start; i < tokens.Count; i++)
            {
                string text = tokens[i].Text;
                if ((text == "(" || text == "[") && matching[i] >= 0)
                {
                    i = matching[i];
                    continue;
                }
                if (text == ";")
                {
                    return i;
                }
                if (text != "{")
                {
                    continue;
                }
                if (matching[i] < 0)
                {
                    return i;
                }
                if (semicolonTerminated)
                {
                    i = matching[i];
                    continue;
                }
                if (i > start && tokens[i - 1].Text == "!" &&
                    !IsMacroInvocationItemPrefix(tokens, matching, start, i))
                {
                    i = matching[i];
                    continue;
                }
                int next = matching[i] + 1;
                if (next < tokens.Count &&
                    (tokens[next].Text == ">" || tokens[next].Text == ","))
                {
                    // A const block inside a generic argument is a balanced
                    // token tree, not the attributed item's body.
                    i = matching[i];
                    continue;
                }
                return matching[i];
            }
            return -1;
        }

        private static bool IsSemicolonTerminatedAttributedItem(
            List<RustToken> tokens,
            int[] matching,
            int start)
        {
            bool sawSemicolonItemKeyword = false;
            for (int i = start; i < tokens.Count; i++)
            {
                string text = tokens[i].Text;
                if ((text == "(" || text == "[") && matching[i] >= 0)
                {
                    i = matching[i];
                    continue;
                }
                if (text == ";" || text == "{")
                {
                    break;
                }
                if (tokens[i].Kind != RustTokenKind.Identifier)
                {
                    continue;
                }
                if (text == "fn" || text == "trait" || text == "impl" || text == "mod" ||
                    text == "struct" || text == "enum" || text == "union" || text == "macro")
                {
                    return false;
                }
                if (text == "const" || text == "static" || text == "type" || text == "use")
                {
                    sawSemicolonItemKeyword = true;
                }
            }
            return sawSemicolonItemKeyword;
        }

        private static bool IsMacroInvocationItemPrefix(
            List<RustToken> tokens,
            int[] matching,
            int start,
            int openBrace)
        {
            bool sawBang = false;
            for (int i = start; i < openBrace; i++)
            {
                string text = tokens[i].Text;
                if ((text == "(" || text == "[") && matching[i] >= 0)
                {
                    i = matching[i];
                    continue;
                }
                if (text == "!")
                {
                    sawBang = true;
                    continue;
                }
                if (tokens[i].Kind == RustTokenKind.Identifier &&
                    (text == "fn" || text == "trait" || text == "impl" || text == "mod" ||
                     text == "struct" || text == "enum" || text == "union" || text == "macro"))
                {
                    return false;
                }
            }
            return sawBang;
        }

        private static List<TokenRange> MergeRanges(List<TokenRange> ranges)
        {
            List<TokenRange> merged = new List<TokenRange>();
            foreach (TokenRange range in ranges)
            {
                if (merged.Count == 0 || range.Start > merged[merged.Count - 1].End + 1)
                {
                    merged.Add(new TokenRange(range.Start, range.End));
                }
                else if (range.End > merged[merged.Count - 1].End)
                {
                    merged[merged.Count - 1].End = range.End;
                }
            }
            return merged;
        }

        private static bool TryFindFunctionEnd(
            List<RustToken> tokens,
            int[] matching,
            int start,
            out int signatureEnd,
            out int endToken)
        {
            int parenDepth = 0;
            int bracketDepth = 0;
            int angleDepth = 0;
            for (int i = start; i < tokens.Count; i++)
            {
                string text = tokens[i].Text;
                if (text == "(")
                {
                    parenDepth++;
                }
                else if (text == ")" && parenDepth > 0)
                {
                    parenDepth--;
                }
                else if (text == "[")
                {
                    bracketDepth++;
                }
                else if (text == "]" && bracketDepth > 0)
                {
                    bracketDepth--;
                }
                else if (text == "<" && parenDepth == 0 && bracketDepth == 0)
                {
                    angleDepth++;
                }
                else if (text == ">" && angleDepth > 0 && parenDepth == 0 && bracketDepth == 0)
                {
                    angleDepth--;
                }
                else if (text == ";" && parenDepth == 0 && bracketDepth == 0 && angleDepth == 0)
                {
                    signatureEnd = i;
                    endToken = i;
                    return true;
                }
                else if (text == "{")
                {
                    if (parenDepth == 0 && bracketDepth == 0 && angleDepth == 0 &&
                        i > 0 && tokens[i - 1].Text == "!" && matching[i] >= 0)
                    {
                        i = matching[i];
                        continue;
                    }
                    if (parenDepth == 0 && bracketDepth == 0 && angleDepth == 0)
                    {
                        if (matching[i] < 0)
                        {
                            break;
                        }
                        signatureEnd = i;
                        endToken = matching[i];
                        return true;
                    }
                    if (matching[i] >= 0)
                    {
                        i = matching[i];
                    }
                }
            }

            signatureEnd = -1;
            endToken = -1;
            return false;
        }

        private static string NormalizeIdentifier(string identifier)
        {
            return identifier.StartsWith("r#", StringComparison.Ordinal)
                ? identifier.Substring(2)
                : identifier;
        }

        private static int FindDeclarationStart(List<RustToken> tokens, int[] matching, int fnIndex)
        {
            int start = fnIndex;
            int cursor = fnIndex - 1;
            while (cursor >= 0)
            {
                RustToken token = tokens[cursor];
                if (token.Kind == RustTokenKind.Identifier &&
                    (token.Text == "pub" || token.Text == "async" || token.Text == "const" ||
                     token.Text == "unsafe" || token.Text == "extern" || token.Text == "default"))
                {
                    start = cursor;
                    cursor--;
                    continue;
                }

                if (token.Kind == RustTokenKind.Literal && cursor > 0 && tokens[cursor - 1].Text == "extern")
                {
                    start = cursor - 1;
                    cursor -= 2;
                    continue;
                }

                if (token.Text == ")" && matching[cursor] > 0)
                {
                    int openParen = matching[cursor];
                    if (tokens[openParen - 1].Text == "pub")
                    {
                        start = openParen - 1;
                        cursor = openParen - 2;
                        continue;
                    }
                }

                if (token.Text == "]" && matching[cursor] > 0)
                {
                    int openBracket = matching[cursor];
                    if (tokens[openBracket - 1].Text == "#")
                    {
                        start = openBracket - 1;
                        cursor = openBracket - 2;
                        continue;
                    }
                }
                break;
            }
            return start;
        }

        private static string HashSignature(List<RustToken> tokens, int start, int end)
        {
            StringBuilder normalized = new StringBuilder();
            for (int i = start; i <= end; i++)
            {
                normalized.Append(tokens[i].Text);
                normalized.Append('\u001f');
            }

            byte[] bytes = Encoding.UTF8.GetBytes(normalized.ToString());
            using (SHA256 sha = SHA256.Create())
            {
                byte[] digest = sha.ComputeHash(bytes);
                StringBuilder hex = new StringBuilder(digest.Length * 2);
                foreach (byte value in digest)
                {
                    hex.Append(value.ToString("x2"));
                }
                return hex.ToString();
            }
        }

        private static string HashLiteral(string source, int start, int length)
        {
            byte[] bytes = Encoding.UTF8.GetBytes(source.Substring(start, length));
            using (SHA256 sha = SHA256.Create())
            {
                byte[] digest = sha.ComputeHash(bytes);
                StringBuilder hex = new StringBuilder(9 + digest.Length * 2);
                hex.Append("literal:");
                foreach (byte value in digest)
                {
                    hex.Append(value.ToString("x2"));
                }
                return hex.ToString();
            }
        }
    }
}
'@
}

function Invoke-ScannerSelfTest {
  Invoke-RustGuardWorkspaceDiscoverySelfTest -Encoding $StrictUtf8 -CodeTargetAssertion {
    param($File, $Content, $HelperFile, $HelperContent)

    $targetFunctions = @([Stage0.RustFunctionScanner]::Scan($Content))
    $longTarget = @($targetFunctions | Where-Object { $_.Name -ceq 'target_long' })
    $helperFunctions = @([Stage0.RustFunctionScanner]::Scan($HelperContent))
    $longHelper = @($helperFunctions | Where-Object { $_.Name -ceq 'helper_long' })
    if ($longTarget.Count -ne 1 -or $longTarget[0].Lines -ne 107 -or
        $longHelper.Count -ne 1 -or $longHelper[0].Lines -ne 107) {
      throw 'Function guard did not scan the 107-line functions in the non-.rs Cargo target and recursive helper module'
    }
  }

  $fixture = @'
fn literals_and_nesting() {
    let normal = "a } brace { and escaped quote \"";
    let bytes = b"} {";
    let raw = r###"} /* not a comment */ {"###;
    let raw_bytes = br##"{ // still literal }"##;
    let character = '}';
    let unicode_character = '\u{007d}';
    // A comment cannot close the function: }
    /* Nor can a nested block comment { /* } */ }. */
    let closure = || { if true { 1 } else { 2 } };
    wrapper!({ { closure() } });
    let after_nested_macro = 1;
}

pub(crate)
async fn async_job(
    value: usize,
) -> usize {
    value
}

unsafe extern "C" fn ffi_job(value: usize) -> usize {
    value
}

struct ConstAssert<const VALUE: bool>;
trait ConstTrue {}
impl ConstTrue for ConstAssert<true> {}

fn const_generic<const N: usize>() -> usize
where
    ConstAssert<{ N < 64 }>: ConstTrue,
{
    N // const-generic-body
}

fn macro_type_signature() -> type_macro! { usize } {
    9 // macro-type-body
}

trait ScannerFixture {
    fn declaration(
        &self,
        value: usize,
    ) -> usize;

    fn default_method(&self) -> usize {
        7
    }
}

struct Fixture(usize);

impl Fixture {
    pub(crate) fn crate_visible(
        &self,
    ) -> usize {
        self.0
    }
}

#[cfg(test)]
mod tests {
    fn hidden_inline_test() {
        panic!("not production");
    }
}

#[cfg(test)]
fn hidden_test_helper() {
    panic!("not production");
}

#[cfg(any(test))]
fn hidden_any_test() {
    panic!("not production");
}

#[cfg(all(test, feature = "scanner-self-test"))]
fn hidden_all_test() {
    panic!("not production");
}

#[cfg(not(test))]
fn visible_not_test() {}

#[cfg(any(test, feature = "scanner-self-test"))]
fn visible_mixed_cfg() {}
'@

  $records = @([Stage0.RustFunctionScanner]::Scan($fixture))
  $expected = @(
    'literals_and_nesting',
    'async_job',
    'ffi_job',
    'const_generic',
    'macro_type_signature',
    'declaration',
    'default_method',
    'crate_visible',
    'visible_not_test',
    'visible_mixed_cfg'
  )
  $actual = @($records | ForEach-Object { $_.Name })
  if (($actual -join ',') -ne ($expected -join ',')) {
    throw "Rust function scanner self-test returned unexpected functions: $($actual -join ', ')"
  }

  $lines = @($fixture -split "`r?`n")
  $outerEnd = 0
  for ($index = 0; $index -lt $lines.Count; $index++) {
    if ($lines[$index] -eq '}') {
      $outerEnd = $index + 1
      break
    }
  }
  $outer = @($records | Where-Object { $_.Name -eq 'literals_and_nesting' })[0]
  if ($outer.EndLine -ne $outerEnd -or $outer.Lines -ne $outerEnd) {
    throw "Literal/comment/nested-brace fixture ended at line $($outer.EndLine), expected $outerEnd"
  }
  if (@($records | Where-Object { $_.Name -like 'hidden*' }).Count -ne 0) {
    throw 'cfg(test) inline items were not excluded by the Rust function scanner'
  }
  if (@($records | Where-Object { $_.SignatureHash -notmatch '^[0-9a-f]{64}$' }).Count -ne 0) {
    throw 'Rust function scanner emitted an invalid normalized signature hash'
  }
  if (@($records | Where-Object { $_.Lines -le 0 -or $_.EndLine -lt $_.StartLine }).Count -ne 0) {
    throw 'Rust function scanner emitted an invalid source span'
  }
  $asyncStart = 1 + [array]::IndexOf($lines, 'pub(crate)')
  $asyncRecord = @($records | Where-Object { $_.Name -eq 'async_job' })[0]
  if ($asyncRecord.StartLine -ne $asyncStart) {
    throw "Rust function span started at line $($asyncRecord.StartLine), expected declaration modifier line $asyncStart"
  }
  $constBody = 1 + [array]::IndexOf($lines, '    N // const-generic-body')
  $constRecord = @($records | Where-Object { $_.Name -eq 'const_generic' })[0]
  if ($constRecord.EndLine -ne $constBody + 1) {
    throw "Const-generic comparison corrupted function end: found $($constRecord.EndLine), expected $($constBody + 1)"
  }
  $macroTypeBody = 1 + [array]::IndexOf($lines, '    9 // macro-type-body')
  $macroTypeRecord = @($records | Where-Object { $_.Name -eq 'macro_type_signature' })[0]
  if ($macroTypeRecord.EndLine -ne $macroTypeBody + 1) {
    throw "Brace-delimited type macro corrupted function end: found $($macroTypeRecord.EndLine), expected $($macroTypeBody + 1)"
  }
  $notTestAttribute = 1 + [array]::IndexOf($lines, '#[cfg(not(test))]')
  $notTestRecord = @($records | Where-Object { $_.Name -eq 'visible_not_test' })[0]
  if ($notTestRecord.StartLine -ne $notTestAttribute) {
    throw 'cfg(not(test)) was excluded or its declaration attribute was not included in the function span'
  }
  $ffi = @($records | Where-Object { $_.Name -eq 'ffi_job' })[0]
  $abiVariant = @([Stage0.RustFunctionScanner]::Scan($fixture.Replace('extern "C"', 'extern "system"'))) |
    Where-Object { $_.Name -eq 'ffi_job' }
  if ($ffi.SignatureHash -eq $abiVariant[0].SignatureHash) {
    throw 'Rust function identity did not detect an extern ABI signature change'
  }
  $formatVariant = @([Stage0.RustFunctionScanner]::Scan($fixture.Replace('unsafe extern', "unsafe`nextern"))) |
    Where-Object { $_.Name -eq 'ffi_job' }
  if ($ffi.SignatureHash -ne $formatVariant[0].SignatureHash) {
    throw 'Rust function identity changed after whitespace-only signature formatting'
  }

  $longBody = ((1..105 | ForEach-Object { "    let value_$($_) = $($_);" }) -join "`n")
  $comparisonAttack = "#[cfg(test)]`nconst T: bool = 1 < 2;`nfn production_after_comparison() {`n$longBody`n}`n"
  $comparisonRecords = @([Stage0.RustFunctionScanner]::Scan($comparisonAttack))
  if ($comparisonRecords.Count -ne 1 -or
      $comparisonRecords[0].Name -cne 'production_after_comparison' -or
      $comparisonRecords[0].Lines -ne 107) {
    throw 'A cfg(test) comparison item swallowed the following 107-line production function'
  }

  $attributedItems = @'
struct FixtureItem;
struct GenericItem<const VALUE: usize>;
trait HiddenTrait { fn hidden_trait_method(&self); }
#[cfg(test)]
static HIDDEN_STATIC: usize = 1;
fn after_static() {}
#[cfg(test)]
const HIDDEN_BLOCK: usize = { 1 };
fn after_const() {}
#[cfg(test)]
type HiddenAlias<T> = Option<T>;
fn after_type() {}
#[cfg(test)]
hidden_cases! { fn hidden_macro_case() {} }
fn after_macro() {}
#[cfg(test)]
trait HiddenOnlyTrait { fn hidden_default() {} }
fn after_trait() {}
#[cfg(test)]
impl HiddenTrait for FixtureItem { fn hidden_trait_method(&self) {} }
fn after_impl() {}
#[cfg(test)]
impl HiddenTrait for GenericItem<{ 1 }> { fn hidden_trait_method(&self) {} }
fn after_const_generic_impl() {}
#[cfg(test)]
const fn hidden_const_fn() -> usize { 1 }
fn after_const_fn() {}
'@
  $attributedNames = @([Stage0.RustFunctionScanner]::Scan($attributedItems) | ForEach-Object { $_.Name })
  $expectedAttributedNames = @(
    'hidden_trait_method',
    'after_static',
    'after_const',
    'after_type',
    'after_macro',
    'after_trait',
    'after_impl',
    'after_const_generic_impl',
    'after_const_fn'
  )
  if (($attributedNames -join ',') -cne ($expectedAttributedNames -join ',')) {
    throw "Attributed const/static/type/macro/trait/impl termination returned unexpected functions: $($attributedNames -join ', ')"
  }

  $hashA = 'a' * 64
  $hashB = 'b' * 64
  $referenceText = "path,name,signatureHash,occurrence,lines`ncrates/sample/src/lib.rs,alpha,$hashA,1,140`ncrates/sample/src/lib.rs,beta,$hashB,1,120"
  $allowedText = "path,name,signatureHash,occurrence,lines`ncrates/sample/src/lib.rs,alpha,$hashA,1,130"
  $addedText = "path,name,signatureHash,occurrence,lines`ncrates/sample/src/lib.rs,alpha,$hashA,1,130`ncrates/sample/src/lib.rs,beta,$hashB,1,120`ncrates/sample/src/lib.rs,gamma,$hashA,1,110"
  $increasedText = "path,name,signatureHash,occurrence,lines`ncrates/sample/src/lib.rs,alpha,$hashA,1,141`ncrates/sample/src/lib.rs,beta,$hashB,1,120"
  $movedText = "path,name,signatureHash,occurrence,lines`ncrates/renamed/src/lib.rs,alpha,$hashA,1,130"
  $referenceBaseline = ConvertFrom-FunctionBaselineText -Content $referenceText -Source 'self-test reference'
  $allowedBaseline = ConvertFrom-FunctionBaselineText -Content $allowedText -Source 'self-test allowed'
  if (@(Get-BaselineTransitionFailures -CurrentBaseline $allowedBaseline -ReferenceBaseline $referenceBaseline).Count -ne 0) {
    throw 'Function-size baseline transition rejected an allowed decrease/deletion'
  }
  foreach ($case in @(
    (ConvertFrom-FunctionBaselineText -Content $addedText -Source 'self-test added'),
    (ConvertFrom-FunctionBaselineText -Content $increasedText -Source 'self-test increased'),
    (ConvertFrom-FunctionBaselineText -Content $movedText -Source 'self-test moved')
  )) {
    if (@(Get-BaselineTransitionFailures -CurrentBaseline $case -ReferenceBaseline $referenceBaseline).Count -eq 0) {
      throw 'Function-size baseline transition accepted an added, increased, or moved identity'
    }
  }

  $emptyBaseline = ConvertFrom-FunctionBaselineText -Content 'path,name,signatureHash,occurrence,lines' -Source 'self-test empty'
  if (@($emptyBaseline.Rows).Count -ne 0 -or
      @(Get-BaselineTransitionFailures -CurrentBaseline $emptyBaseline -ReferenceBaseline $referenceBaseline).Count -ne 0) {
    throw 'Function-size baseline did not accept a header-only zero-debt transition'
  }
  if ((@((Write-FunctionBaselineCsv -Rows @())) -join "`n") -cne 'path,name,signatureHash,occurrence,lines') {
    throw 'Function-size baseline generator did not emit a header-only zero-debt baseline'
  }

  foreach ($invalidCase in @(
    "path,name,signatureHash,occurrence,lines,extra`ncrates/sample/src/lib.rs,alpha,$hashA,1,140,hidden",
    "path,name,signatureHash,occurrence,lines`ncrates/sample/src/lib.rs,alpha,$hashA,+1,140",
    "path,name,signatureHash,occurrence,lines`ncrates/sample/src/lib.rs,alpha,$hashA,01,140"
  )) {
    $rejected = $false
    try {
      [void](ConvertFrom-FunctionBaselineText -Content $invalidCase -Source 'self-test strict CSV')
    } catch {
      $rejected = $true
    }
    if (-not $rejected) {
      throw 'Function-size baseline accepted an extra column or non-canonical integer'
    }
  }

  $caseChangedText = "path,name,signatureHash,occurrence,lines`ncrates/Sample/src/lib.rs,alpha,$hashA,1,130"
  $caseChangedBaseline = ConvertFrom-FunctionBaselineText -Content $caseChangedText -Source 'self-test case change'
  if (@(Get-BaselineTransitionFailures -CurrentBaseline $caseChangedBaseline -ReferenceBaseline $referenceBaseline).Count -eq 0) {
    throw 'Function-size baseline transition accepted a case-only identity change'
  }

  $tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('meow-function-bootstrap-' + [guid]::NewGuid().ToString('N'))
  try {
    [void][System.IO.Directory]::CreateDirectory($tempRoot)
    $tempBaseline = Join-Path $tempRoot 'baseline.csv'
    $tempManifest = Join-Path $tempRoot 'bootstrap.csv'
    [System.IO.File]::WriteAllText($tempBaseline, $allowedText + "`n", $StrictUtf8)
    $tempHash = Get-Sha256Hex -Bytes ([System.IO.File]::ReadAllBytes($tempBaseline))
    [System.IO.File]::WriteAllText(
      $tempManifest,
      "referenceRevision,baselineSha256`n$InitialBootstrapReference,$tempHash`n",
      $StrictUtf8
    )
    Assert-BootstrapManifest `
      -ReferenceCommit $InitialBootstrapReference `
      -ExpectedSha256 $tempHash `
      -ManifestPath $tempManifest `
      -BaselineFilePath $tempBaseline

    $wrongReferenceRejected = $false
    try {
      Assert-BootstrapManifest `
        -ReferenceCommit ('1' * 40) `
        -ExpectedSha256 $tempHash `
        -ManifestPath $tempManifest `
        -BaselineFilePath $tempBaseline
    } catch {
      $wrongReferenceRejected = $_.Exception.Message -match 'only authorized against'
    }
    if (-not $wrongReferenceRejected) {
      throw 'Function-size bootstrap accepted a non-fixed initial revision'
    }
    $untrustedBootstrapRejected = $false
    try {
      Assert-BootstrapManifest `
        -ReferenceCommit $InitialBootstrapReference `
        -ExpectedSha256 ('0' * 64) `
        -ManifestPath $tempManifest `
        -BaselineFilePath $tempBaseline
    } catch {
      $untrustedBootstrapRejected = $_.Exception.Message -match 'trusted SHA-256'
    }
    if (-not $untrustedBootstrapRejected) {
      throw 'Function-size bootstrap accepted a PR-controlled manifest without trusted authorization'
    }
  } finally {
    if (Test-Path -LiteralPath $tempRoot) {
      Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
  }

  Write-Host ('Rust function scanner self-test passed: functions={0}, trusted bootstrap, case-safe physical identity, taskkill/job/snapshot tree cleanup, normal-process isolation, non-.rs target recursion, path/include rejection, attributed items, cfg, identity, and transitions covered' -f $records.Count)
}

function Get-AllProductionFunctions {
  $rows = @()
  foreach ($file in (Get-ProductionRustFiles)) {
    $relative = Get-NormalizedRelativePath -FullName $file.FullName
    $content = Read-StrictUtf8Text -Path $file.FullName
    foreach ($function in [Stage0.RustFunctionScanner]::Scan($content)) {
      $rows += [PSCustomObject]@{
        path = $relative
        name = $function.Name
        signatureHash = $function.SignatureHash
        occurrence = $function.Occurrence
        startLine = $function.StartLine
        lines = $function.Lines
      }
    }
  }

  return @(Sort-FunctionRowsOrdinal -Rows $rows)
}

function Get-FunctionKey {
  param([Parameter(Mandatory = $true)]$Row)

  return '{0}|{1}|{2}|{3}' -f $Row.path, $Row.name, $Row.signatureHash, $Row.occurrence
}

function Get-FunctionSortKey {
  param([Parameter(Mandatory = $true)]$Row)

  $separator = [char]0
  return '{0}{4}{1}{4}{2}{4}{3:D8}' -f
    $Row.path,
    $Row.name,
    $Row.signatureHash,
    [int]$Row.occurrence,
    $separator
}

function Sort-FunctionRowsOrdinal {
  param([AllowEmptyCollection()][array]$Rows = @())

  $sorted = New-Object System.Collections.ArrayList
  foreach ($row in $Rows) {
    $key = Get-FunctionSortKey -Row $row
    $low = 0
    $high = $sorted.Count
    while ($low -lt $high) {
      $middle = $low + [int][Math]::Floor(($high - $low) / 2.0)
      $middleKey = Get-FunctionSortKey -Row $sorted[$middle]
      if ([string]::CompareOrdinal($middleKey, $key) -lt 0) {
        $low = $middle + 1
      } else {
        $high = $middle
      }
    }
    [void]$sorted.Insert($low, $row)
  }
  return @($sorted)
}

function ConvertTo-RequiredInt {
  param(
    [Parameter(Mandatory = $true)]$Value,
    [Parameter(Mandatory = $true)][string]$Field,
    [Parameter(Mandatory = $true)][string]$Identity
  )

  return ConvertTo-RustGuardCanonicalInt -Value $Value -Field $Field -Identity $Identity
}

function Write-FunctionBaselineCsv {
  param([Parameter(Mandatory = $true)][AllowEmptyCollection()][array]$Rows)

  Write-Output 'path,name,signatureHash,occurrence,lines'
  foreach ($row in (Sort-FunctionRowsOrdinal -Rows $Rows)) {
    Write-Output ('{0},{1},{2},{3},{4}' -f
      (Format-RustGuardCsvField -Value ([string]$row.path)),
      (Format-RustGuardCsvField -Value ([string]$row.name)),
      [string]$row.signatureHash,
      [int]$row.occurrence,
      [int]$row.lines)
  }
}

function ConvertFrom-FunctionBaselineText {
  param(
    [Parameter(Mandatory = $true)][string]$Content,
    [Parameter(Mandatory = $true)][string]$Source
  )

  $header = 'path,name,signatureHash,occurrence,lines'
  $rows = @(ConvertFrom-RustGuardCsv -Content $Content -Header $header -Source $Source)
  $byKey = New-RustGuardOrdinalDictionary
  $previousSortKey = $null
  foreach ($entry in $rows) {
    foreach ($field in @('path', 'name', 'signatureHash', 'occurrence', 'lines')) {
      if ([string]::IsNullOrWhiteSpace([string]$entry.$field)) {
        throw "Function-size baseline at $Source contains an empty required field '$field'"
      }
    }
    if (-not (Test-RustGuardNormalizedRepositoryPath -Path ([string]$entry.path))) {
      throw "Function-size baseline path at $Source must be normalized and repository-relative: $($entry.path)"
    }
    if (-not (Test-RustGuardProductionRepositoryPath -Path ([string]$entry.path))) {
      throw "Excluded source must not appear in the function-size baseline at ${Source}: $($entry.path)"
    }
    if ($entry.signatureHash -notmatch '^[0-9a-f]{64}$') {
      throw "Function-size baseline at $Source contains an invalid signature hash: $($entry.path)::$($entry.name)"
    }

    $occurrence = ConvertTo-RequiredInt -Value $entry.occurrence -Field 'occurrence' -Identity "$($entry.path)::$($entry.name)"
    $lines = ConvertTo-RequiredInt -Value $entry.lines -Field 'lines' -Identity "$($entry.path)::$($entry.name)"
    if ($lines -le $TargetLines) {
      throw "Function-size baseline lines at $Source must exceed the ${TargetLines}-line target: $($entry.path)::$($entry.name)=$lines"
    }
    $entry.occurrence = $occurrence
    $entry.lines = $lines

    $key = Get-FunctionKey -Row $entry
    if ($byKey.ContainsKey($key)) {
      throw "Function-size baseline at $Source contains a duplicate identity: $key"
    }
    $sortKey = Get-FunctionSortKey -Row $entry
    if ($null -ne $previousSortKey -and [string]::CompareOrdinal($previousSortKey, $sortKey) -ge 0) {
      throw "Function-size baseline at $Source is not in deterministic path/name/signatureHash/occurrence order near: $key"
    }
    $previousSortKey = $sortKey
    $byKey.Add($key, $entry)
  }

  $canonicalRows = @()
  foreach ($entry in $rows) {
    $canonicalRows += [PSCustomObject]@{
      path = [string]$entry.path
      name = [string]$entry.name
      signatureHash = [string]$entry.signatureHash
      occurrence = [int]$entry.occurrence
      lines = [int]$entry.lines
    }
  }
  $canonical = @((Write-FunctionBaselineCsv -Rows $canonicalRows)) -join "`n"
  Assert-RustGuardCanonicalCsvText -Content $Content -Canonical $canonical -Source $Source

  return [PSCustomObject]@{
    Rows = [object[]]$rows
    ByKey = $byKey
  }
}

function Get-RepositoryRelativeFilePath {
  param([Parameter(Mandatory = $true)][string]$Path)

  $fullPath = [System.IO.Path]::GetFullPath($Path)
  $rootPrefix = $repoRoot.TrimEnd([char[]]@('\', '/')) + [System.IO.Path]::DirectorySeparatorChar
  if (-not $fullPath.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Baseline transition files must remain inside the repository: $fullPath"
  }
  return Get-NormalizedRelativePath -FullName $fullPath
}

function Invoke-GitCapture {
  param([Parameter(Mandatory = $true)][string[]]$Arguments)

  $previousPreference = $ErrorActionPreference
  try {
    $ErrorActionPreference = 'Continue'
    $output = @(& git @Arguments 2>&1)
    $exitCode = $LASTEXITCODE
  } finally {
    $ErrorActionPreference = $previousPreference
  }
  return [PSCustomObject]@{
    ExitCode = $exitCode
    Output = @($output | ForEach-Object { [string]$_ })
  }
}

function Resolve-ReferenceCommit {
  $candidate = $ReferenceRevision
  if ([string]::IsNullOrWhiteSpace($candidate)) {
    $candidate = [Environment]::GetEnvironmentVariable('RUST_FUNCTION_SIZE_BASELINE_REFERENCE')
  }
  if ([string]::IsNullOrWhiteSpace($candidate)) {
    $candidate = 'HEAD'
  }

  $result = Invoke-GitCapture -Arguments @('-C', $repoRoot, 'rev-parse', '--verify', "${candidate}^{commit}")
  if ($result.ExitCode -ne 0 -or $result.Output.Count -ne 1 -or $result.Output[0] -notmatch '^[0-9a-fA-F]{40}$') {
    throw "Unable to resolve function-size baseline reference revision '$candidate': $($result.Output -join ' ')"
  }
  return ([string]$result.Output[0]).ToLowerInvariant()
}

function Get-GitFileAtRevision {
  param(
    [Parameter(Mandatory = $true)][string]$Revision,
    [Parameter(Mandatory = $true)][string]$RepositoryPath
  )

  $object = "${Revision}:${RepositoryPath}"
  $existsResult = Invoke-GitCapture -Arguments @('-C', $repoRoot, 'cat-file', '-e', $object)
  if ($existsResult.ExitCode -ne 0) {
    return [PSCustomObject]@{
      Exists = $false
      Content = $null
    }
  }

  $showResult = Invoke-GitCapture -Arguments @('-C', $repoRoot, 'show', $object)
  if ($showResult.ExitCode -ne 0) {
    throw "Unable to read $RepositoryPath from reference revision ${Revision}: $($showResult.Output -join ' ')"
  }
  return [PSCustomObject]@{
    Exists = $true
    Content = ($showResult.Output -join "`n")
  }
}

function Get-Sha256Hex {
  param([Parameter(Mandatory = $true)][byte[]]$Bytes)

  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    $digest = $sha.ComputeHash($Bytes)
  } finally {
    $sha.Dispose()
  }
  return (($digest | ForEach-Object { $_.ToString('x2') }) -join '')
}

function Assert-BootstrapManifest {
  param(
    [Parameter(Mandatory = $true)][string]$ReferenceCommit,
    [AllowEmptyString()][string]$ExpectedSha256 = $TrustedBootstrapSha256,
    [string]$ManifestPath = $BootstrapManifestPath,
    [string]$BaselineFilePath = $BaselinePath
  )

  if ($ReferenceCommit -cne $InitialBootstrapReference) {
    throw "Function-size bootstrap is only authorized against $InitialBootstrapReference, not $ReferenceCommit"
  }
  if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) {
    throw "Function-size baseline does not exist at reference $ReferenceCommit and requires the explicit one-time bootstrap manifest: $ManifestPath"
  }
  $content = Read-StrictUtf8Text -Path $ManifestPath
  $header = 'referenceRevision,baselineSha256'
  $rows = @(ConvertFrom-RustGuardCsv -Content $content -Header $header -Source $ManifestPath)
  if ($rows.Count -ne 1) {
    throw 'Function-size bootstrap manifest must contain exactly one authorization row'
  }
  $row = $rows[0]
  if ([string]$row.referenceRevision -notmatch '^[0-9a-f]{40}$' -or
      [string]$row.referenceRevision -cne $InitialBootstrapReference -or
      [string]$row.referenceRevision -cne $ReferenceCommit) {
    throw "Function-size bootstrap manifest must authorize exactly $InitialBootstrapReference"
  }
  if ([string]$row.baselineSha256 -notmatch '^[0-9a-f]{64}$') {
    throw 'Function-size bootstrap manifest contains an invalid baselineSha256'
  }

  $canonical = "$header`n$($row.referenceRevision),$($row.baselineSha256)"
  Assert-RustGuardCanonicalCsvText -Content $content -Canonical $canonical -Source $ManifestPath

  $actualHash = Get-Sha256Hex -Bytes ([System.IO.File]::ReadAllBytes($BaselineFilePath))
  if ($actualHash -cne [string]$row.baselineSha256) {
    throw "Function-size bootstrap manifest does not authorize the current baseline bytes: expected $($row.baselineSha256), found $actualHash"
  }
  Assert-RustGuardTrustedBootstrapSha256 `
    -GuardName 'Function-size' `
    -ExpectedSha256 $ExpectedSha256 `
    -ManifestSha256 ([string]$row.baselineSha256) `
    -ActualSha256 $actualHash
}

function Get-BaselineTransitionFailures {
  param(
    [Parameter(Mandatory = $true)]$CurrentBaseline,
    [Parameter(Mandatory = $true)]$ReferenceBaseline
  )

  $failures = @()
  foreach ($entry in $CurrentBaseline.Rows) {
    $key = Get-FunctionKey -Row $entry
    if (-not $ReferenceBaseline.ByKey.ContainsKey($key)) {
      $failures += "baseline transition added or changed identity/path: $($entry.path)::$($entry.name)"
      continue
    }
    $referenceLines = [int]$ReferenceBaseline.ByKey[$key].lines
    if ([int]$entry.lines -gt $referenceLines) {
      $failures += "baseline transition increased allowance: $($entry.path)::$($entry.name) from $referenceLines to $($entry.lines)"
    }
  }
  return $failures
}

function Assert-BaselineTransition {
  param(
    [Parameter(Mandatory = $true)]$CurrentBaseline,
    [Parameter(Mandatory = $true)][string]$ReferenceCommit
  )

  $baselineRepoPath = Get-RepositoryRelativeFilePath -Path $BaselinePath
  $referenceFile = Get-GitFileAtRevision -Revision $ReferenceCommit -RepositoryPath $baselineRepoPath
  if (-not $referenceFile.Exists) {
    Assert-BootstrapManifest -ReferenceCommit $ReferenceCommit
    Write-Host "Rust function baseline transition: explicit bootstrap authorized against $ReferenceCommit"
    return
  }

  $referenceBaseline = ConvertFrom-FunctionBaselineText -Content $referenceFile.Content -Source "$ReferenceCommit`:$baselineRepoPath"
  $transitionFailures = @(Get-BaselineTransitionFailures -CurrentBaseline $CurrentBaseline -ReferenceBaseline $referenceBaseline)
  if ($transitionFailures.Count -gt 0) {
    throw "Function-size baseline transition rejected against ${ReferenceCommit}:`n$($transitionFailures -join "`n")"
  }

  Write-Host ('Rust function baseline transition passed: reference={0}, reference rows={1}, current rows={2}; only decreases/deletions allowed' -f $ReferenceCommit, $referenceBaseline.Rows.Count, $CurrentBaseline.Rows.Count)
}

if ($SelfTest) {
  Invoke-ScannerSelfTest
  return
}

$allFunctions = @(Get-AllProductionFunctions)
$current = @($allFunctions | Where-Object { $_.lines -gt $TargetLines })

if ($GenerateBaseline) {
  Write-FunctionBaselineCsv -Rows $current
  return
}

if (-not (Test-Path -LiteralPath $BaselinePath -PathType Leaf)) {
  throw "Function-size baseline is missing: $BaselinePath"
}
$baselineDocument = ConvertFrom-FunctionBaselineText -Content (Read-StrictUtf8Text -Path $BaselinePath) -Source $BaselinePath
$baseline = @($baselineDocument.Rows)
$baselineByKey = $baselineDocument.ByKey
$referenceCommit = Resolve-ReferenceCommit
Assert-BaselineTransition -CurrentBaseline $baselineDocument -ReferenceCommit $referenceCommit

$allByKey = New-RustGuardOrdinalDictionary
foreach ($row in $allFunctions) {
  $key = Get-FunctionKey -Row $row
  if ($allByKey.ContainsKey($key)) {
    throw "Rust function scanner produced a duplicate identity: $key"
  }
  $allByKey.Add($key, $row)
}

$currentByKey = New-RustGuardOrdinalDictionary
foreach ($row in $current) {
  $key = Get-FunctionKey -Row $row
  if ($currentByKey.ContainsKey($key)) {
    throw "Rust function violation scan produced a duplicate identity: $key"
  }
  $currentByKey.Add($key, $row)
}

$failures = @()
foreach ($row in $current) {
  $key = Get-FunctionKey -Row $row
  $location = '{0}:{1}::{2}' -f $row.path, $row.startLine, $row.name
  if (-not $baselineByKey.ContainsKey($key)) {
    if ($row.lines -gt $HardLines) {
      $failures += 'new hard-ceiling violation: {0} has {1} lines (target {2}, new-code hard ceiling {3}) and no exact baseline identity' -f $location, $row.lines, $TargetLines, $HardLines
    } else {
      $failures += 'new violation: {0} has {1} lines (target {2}) and no exact baseline identity' -f $location, $row.lines, $TargetLines
    }
    continue
  }
  $baselineLines = [int]$baselineByKey[$key].lines
  if ($row.lines -gt $baselineLines) {
    $failures += 'increased violation: {0} grew from {1} to {2} lines' -f $location, $baselineLines, $row.lines
  }
}

foreach ($entry in $baseline) {
  $key = Get-FunctionKey -Row $entry
  if (-not $allByKey.ContainsKey($key)) {
    $failures += 'stale baseline identity (function renamed, moved, removed, or signature changed): {0}::{1}' -f $entry.path, $entry.name
  } elseif (-not $currentByKey.ContainsKey($key)) {
    $failures += 'stale resolved baseline identity (function is now within target; remove the row): {0}::{1}' -f $entry.path, $entry.name
  }
}

if ($failures.Count -gt 0) {
  Write-Error "Rust function size guard failed:`n$($failures -join "`n")"
}

$historicHardDebt = @($current | Where-Object { $_.lines -gt $HardLines }).Count
Write-Host ('Rust function size guard passed: production functions={0}, current violations={1}, baseline violations={2}, historic hard debt (>{4})={3}, thresholds target={5}, new hard={4}' -f $allFunctions.Count, $current.Count, $baseline.Count, $historicHardDebt, $HardLines, $TargetLines)
