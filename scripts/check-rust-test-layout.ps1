#Requires -Version 5.1
<#
.SYNOPSIS
  CI guard: lock Rust tests embedded under src during test migration.
.DESCRIPTION
  Scans crates/*/src and apps/desktop/src-tauri/src for test-layout debt:
  inline #[cfg(test)] modules, #[test] attributes, mod tests { blocks,
  #[cfg(test)] helpers outside inline modules, and physical test-only files
  under src. Existing debt recorded in scripts/baselines may stay or shrink,
  but it must not grow. Reference-revision transition validation prevents a
  baseline edit from authorizing itself. The only accepted post-migration bridge is:
    #[cfg(test)]
    #[path = "../tests/unit/..."]
    mod tests;
  Use -GenerateBaseline to print the current CSV baseline to stdout; the
  script never writes baselines itself.
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
  $BaselinePath = Join-Path $repoRoot 'scripts/baselines/rust-test-layout-baseline.csv'
}
if ([string]::IsNullOrWhiteSpace($BootstrapManifestPath)) {
  $BootstrapManifestPath = Join-Path $repoRoot 'scripts/baselines/rust-test-layout-bootstrap.csv'
}
if ([string]::IsNullOrWhiteSpace($TrustedBootstrapSha256)) {
  $TrustedBootstrapSha256 = $env:RUST_TEST_LAYOUT_BOOTSTRAP_SHA256
}

$StrictUtf8 = [System.Text.UTF8Encoding]::new($false, $true)
$BootstrapReferenceRevision = '2087df1cc5209fa879cdb3796e9a1437196bc2f4'
$MetricFields = @(
  'inlineTestModules',
  'inlineTestModuleLines',
  'testAttributes',
  'modTestsBlocks',
  'testOnlyCfgItems',
  'srcTestFileLines'
)
$BaselineHeader = 'path,inlineTestModules,inlineTestModuleLines,testAttributes,modTestsBlocks,testOnlyCfgItems,srcTestFileLines'

if (-not ('Stage0.RustLexicalMasker' -as [type])) {
  Add-Type -Language CSharp -TypeDefinition @'
using System;
using System.Globalization;

namespace Stage0
{
    public static class RustLexicalMasker
    {
        public static string Mask(string source)
        {
            if (source == null)
            {
                throw new ArgumentNullException("source");
            }

            char[] masked = source.ToCharArray();
            int index = 0;
            while (index < source.Length)
            {
                int end;
                if (source[index] == '/' && index + 1 < source.Length && source[index + 1] == '/')
                {
                    end = index + 2;
                    while (end < source.Length && source[end] != '\r' && source[end] != '\n')
                    {
                        end++;
                    }
                    Blank(masked, index, end);
                    index = end;
                    continue;
                }

                if (source[index] == '/' && index + 1 < source.Length && source[index + 1] == '*')
                {
                    end = FindBlockCommentEnd(source, index);
                    Blank(masked, index, end);
                    index = end;
                    continue;
                }

                if (TryFindRawStringEnd(source, index, out end))
                {
                    Blank(masked, index, end);
                    index = end;
                    continue;
                }

                if ((source[index] == 'b' || source[index] == 'c') &&
                    index + 1 < source.Length && source[index + 1] == '"' &&
                    IsTokenBoundary(source, index))
                {
                    end = FindQuotedStringEnd(source, index + 1);
                    Blank(masked, index, end);
                    index = end;
                    continue;
                }

                if (source[index] == '"')
                {
                    end = FindQuotedStringEnd(source, index);
                    Blank(masked, index, end);
                    index = end;
                    continue;
                }

                if (source[index] == 'b' && index + 1 < source.Length &&
                    source[index + 1] == '\'' && IsTokenBoundary(source, index) &&
                    TryFindCharacterEnd(source, index + 1, out end))
                {
                    Blank(masked, index, end);
                    index = end;
                    continue;
                }

                if (source[index] == '\'' && TryFindCharacterEnd(source, index, out end))
                {
                    Blank(masked, index, end);
                    index = end;
                    continue;
                }

                index++;
            }

            return new string(masked);
        }

        private static int FindBlockCommentEnd(string source, int start)
        {
            int depth = 1;
            int cursor = start + 2;
            while (cursor < source.Length && depth > 0)
            {
                if (cursor + 1 < source.Length && source[cursor] == '/' && source[cursor + 1] == '*')
                {
                    depth++;
                    cursor += 2;
                }
                else if (cursor + 1 < source.Length && source[cursor] == '*' && source[cursor + 1] == '/')
                {
                    depth--;
                    cursor += 2;
                }
                else
                {
                    cursor++;
                }
            }
            return cursor;
        }

        private static bool TryFindRawStringEnd(string source, int start, out int end)
        {
            end = start;
            if (!IsTokenBoundary(source, start))
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
                if (source[cursor] != '"')
                {
                    cursor++;
                    continue;
                }

                int suffix = cursor + 1;
                int seen = 0;
                while (seen < hashes && suffix < source.Length && source[suffix] == '#')
                {
                    suffix++;
                    seen++;
                }
                if (seen == hashes)
                {
                    end = suffix;
                    return true;
                }
                cursor++;
            }

            end = source.Length;
            return true;
        }

        private static int FindQuotedStringEnd(string source, int quote)
        {
            bool escaped = false;
            int cursor = quote + 1;
            while (cursor < source.Length)
            {
                char current = source[cursor];
                if (!escaped && current == '"')
                {
                    return cursor + 1;
                }
                if (!escaped && current == '\\')
                {
                    escaped = true;
                }
                else
                {
                    escaped = false;
                }
                cursor++;
            }
            return source.Length;
        }

        private static bool TryFindCharacterEnd(string source, int quote, out int end)
        {
            end = quote;
            int cursor = quote + 1;
            if (cursor >= source.Length || source[cursor] == '\r' || source[cursor] == '\n')
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
                           source[cursor] != '\r' && source[cursor] != '\n')
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
                    if (cursor > source.Length)
                    {
                        return false;
                    }
                }
                else
                {
                    cursor++;
                }
            }
            else if (Char.IsHighSurrogate(source[cursor]) && cursor + 1 < source.Length &&
                     Char.IsLowSurrogate(source[cursor + 1]))
            {
                cursor += 2;
            }
            else
            {
                cursor++;
            }

            if (cursor < source.Length && source[cursor] == '\'')
            {
                end = cursor + 1;
                return true;
            }
            return false;
        }

        private static bool IsTokenBoundary(string source, int index)
        {
            return index == 0 || !IsIdentifierContinue(source[index - 1]);
        }

        private static bool IsIdentifierContinue(char value)
        {
            UnicodeCategory category = Char.GetUnicodeCategory(value);
            return value == '_' || Char.IsLetterOrDigit(value) ||
                   category == UnicodeCategory.NonSpacingMark ||
                   category == UnicodeCategory.SpacingCombiningMark ||
                   category == UnicodeCategory.ConnectorPunctuation;
        }

        private static void Blank(char[] masked, int start, int end)
        {
            int limit = Math.Min(end, masked.Length);
            for (int index = start; index < limit; index++)
            {
                if (masked[index] != '\r' && masked[index] != '\n')
                {
                    masked[index] = ' ';
                }
            }
        }
    }
}
'@
}

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

function Get-LineCountFromText {
  param([Parameter(Mandatory = $true)][string]$Content)

  if ($Content.Length -eq 0) {
    return 0
  }

  $lineFeeds = ([regex]::Matches($Content, "`n")).Count
  if ($Content.EndsWith("`n")) {
    return $lineFeeds
  }

  return $lineFeeds + 1
}

function Get-RustLexicalMask {
  param([Parameter(Mandatory = $true)][string]$Content)

  $masked = [Stage0.RustLexicalMasker]::Mask($Content)
  if ($masked.Length -ne $Content.Length) {
    throw 'Rust lexical mask did not preserve source length'
  }
  return $masked
}

function Find-MatchingRustDelimiter {
  param(
    [Parameter(Mandatory = $true)][string]$Mask,
    [Parameter(Mandatory = $true)][int]$OpenIndex,
    [Parameter(Mandatory = $true)][char]$Open,
    [Parameter(Mandatory = $true)][char]$Close
  )

  $depth = 0
  for ($index = $OpenIndex; $index -lt $Mask.Length; $index++) {
    if ($Mask[$index] -eq $Open) {
      $depth++
    } elseif ($Mask[$index] -eq $Close) {
      $depth--
      if ($depth -eq 0) {
        return $index
      }
    }
  }
  return -1
}

function Get-NextNonWhitespaceIndex {
  param(
    [Parameter(Mandatory = $true)][string]$Text,
    [Parameter(Mandatory = $true)][int]$Start
  )

  $index = $Start
  while ($index -lt $Text.Length -and [char]::IsWhiteSpace($Text[$index])) {
    $index++
  }
  return $index
}

function Test-RustIdentifierContinue {
  param([Parameter(Mandatory = $true)][char]$Value)

  return $Value -eq '_' -or [char]::IsLetterOrDigit($Value)
}

function Test-RustKeywordAt {
  param(
    [Parameter(Mandatory = $true)][string]$Text,
    [Parameter(Mandatory = $true)][int]$Index,
    [Parameter(Mandatory = $true)][string]$Keyword
  )

  if ($Index -lt 0 -or $Index + $Keyword.Length -gt $Text.Length) {
    return $false
  }
  if ($Text.Substring($Index, $Keyword.Length) -cne $Keyword) {
    return $false
  }
  if ($Index -gt 0 -and (Test-RustIdentifierContinue -Value $Text[$Index - 1])) {
    return $false
  }
  $after = $Index + $Keyword.Length
  return $after -ge $Text.Length -or -not (Test-RustIdentifierContinue -Value $Text[$after])
}

function Get-CfgAttrPayload {
  param([Parameter(Mandatory = $true)][string]$AttributeBody)

  $open = $AttributeBody.IndexOf('(')
  if ($open -lt 0) {
    return $null
  }

  $parenDepth = 1
  $bracketDepth = 0
  $braceDepth = 0
  for ($index = $open + 1; $index -lt $AttributeBody.Length; $index++) {
    $char = $AttributeBody[$index]
    if ($char -eq '(') {
      $parenDepth++
    } elseif ($char -eq ')') {
      $parenDepth--
      if ($parenDepth -eq 0) {
        return $null
      }
    } elseif ($char -eq '[') {
      $bracketDepth++
    } elseif ($char -eq ']' -and $bracketDepth -gt 0) {
      $bracketDepth--
    } elseif ($char -eq '{') {
      $braceDepth++
    } elseif ($char -eq '}' -and $braceDepth -gt 0) {
      $braceDepth--
    } elseif ($char -eq ',' -and $parenDepth -eq 1 -and $bracketDepth -eq 0 -and $braceDepth -eq 0) {
      $close = Find-MatchingRustDelimiter -Mask $AttributeBody -OpenIndex $open -Open '(' -Close ')'
      if ($close -gt $index) {
        return $AttributeBody.Substring($index + 1, $close - $index - 1)
      }
      return $null
    }
  }
  return $null
}

function Get-CfgAttrCondition {
  param([Parameter(Mandatory = $true)][string]$AttributeBody)

  $open = $AttributeBody.IndexOf('(')
  if ($open -lt 0) {
    return $null
  }
  $parenDepth = 1
  $bracketDepth = 0
  $braceDepth = 0
  for ($index = $open + 1; $index -lt $AttributeBody.Length; $index++) {
    $char = $AttributeBody[$index]
    if ($char -eq '(') {
      $parenDepth++
    } elseif ($char -eq ')') {
      $parenDepth--
      if ($parenDepth -eq 0) {
        return $null
      }
    } elseif ($char -eq '[') {
      $bracketDepth++
    } elseif ($char -eq ']' -and $bracketDepth -gt 0) {
      $bracketDepth--
    } elseif ($char -eq '{') {
      $braceDepth++
    } elseif ($char -eq '}' -and $braceDepth -gt 0) {
      $braceDepth--
    } elseif ($char -eq ',' -and $parenDepth -eq 1 -and $bracketDepth -eq 0 -and $braceDepth -eq 0) {
      return $AttributeBody.Substring($open + 1, $index - $open - 1)
    }
  }
  return $null
}

function Get-RustCfgTokens {
  param([Parameter(Mandatory = $true)][string]$Expression)

  $tokens = @()
  foreach ($match in [regex]::Matches($Expression, '[A-Za-z_][A-Za-z0-9_]*|[(),=]')) {
    $tokens += $match.Value
  }
  return $tokens
}

function Read-RustCfgImplication {
  param([Parameter(Mandatory = $true)]$State)

  if ($State.Index -ge $State.Tokens.Count -or
      $State.Tokens[$State.Index] -notmatch '^[A-Za-z_]') {
    return [PSCustomObject]@{ Valid = $false; ImpliesTest = $false }
  }

  $predicate = [string]$State.Tokens[$State.Index]
  $State.Index++
  if ($State.Index -lt $State.Tokens.Count -and $State.Tokens[$State.Index] -ceq '=') {
    $State.Index++
    if ($State.Index -lt $State.Tokens.Count -and
        $State.Tokens[$State.Index] -notin @(',', ')')) {
      $State.Index++
    }
    return [PSCustomObject]@{ Valid = $true; ImpliesTest = $false }
  }

  if ($State.Index -ge $State.Tokens.Count -or $State.Tokens[$State.Index] -cne '(') {
    return [PSCustomObject]@{ Valid = $true; ImpliesTest = ($predicate -ceq 'test') }
  }

  $State.Index++
  $children = @()
  while ($State.Index -lt $State.Tokens.Count -and $State.Tokens[$State.Index] -cne ')') {
    if ($State.Tokens[$State.Index] -ceq ',') {
      $State.Index++
      continue
    }
    $child = Read-RustCfgImplication -State $State
    if (-not $child.Valid) {
      return $child
    }
    $children += [bool]$child.ImpliesTest
    if ($State.Index -lt $State.Tokens.Count -and
        $State.Tokens[$State.Index] -notin @(',', ')')) {
      return [PSCustomObject]@{ Valid = $false; ImpliesTest = $false }
    }
  }
  if ($State.Index -ge $State.Tokens.Count -or $State.Tokens[$State.Index] -cne ')') {
    return [PSCustomObject]@{ Valid = $false; ImpliesTest = $false }
  }
  $State.Index++

  $implies = $false
  if ($predicate -ceq 'all') {
    $implies = $children -contains $true
  } elseif ($predicate -ceq 'any' -and $children.Count -gt 0) {
    $implies = $children -notcontains $false
  }
  return [PSCustomObject]@{ Valid = $true; ImpliesTest = $implies }
}

function Test-RustCfgExpressionImpliesTest {
  param([Parameter(Mandatory = $true)][string]$Expression)

  $tokens = @(Get-RustCfgTokens -Expression $Expression)
  if ($tokens.Count -eq 0) {
    return $false
  }
  $state = [PSCustomObject]@{ Tokens = $tokens; Index = 0 }
  $result = Read-RustCfgImplication -State $state
  return $result.Valid -and $state.Index -eq $tokens.Count -and $result.ImpliesTest
}

function Test-RustCfgAttributeImpliesTest {
  param([Parameter(Mandatory = $true)][string]$AttributeBody)

  $open = $AttributeBody.IndexOf('(')
  if ($open -lt 0) {
    return $false
  }
  $close = Find-MatchingRustDelimiter -Mask $AttributeBody -OpenIndex $open -Open '(' -Close ')'
  if ($close -ne $AttributeBody.Length - 1) {
    return $false
  }
  return Test-RustCfgExpressionImpliesTest -Expression $AttributeBody.Substring($open + 1, $close - $open - 1)
}

function Test-IsKnownTestAttributeImport {
  param([Parameter(Mandatory = $true)][string]$Path)

  return @(
    'tokio::test',
    'async_std::test',
    'rstest::rstest',
    'test_case::test_case',
    'wasm_bindgen_test::wasm_bindgen_test',
    'parameterized::parameterized',
    'quickcheck::quickcheck'
  ) -ccontains $Path
}

function Test-IsKnownTestMacroImport {
  param([Parameter(Mandatory = $true)][string]$Path)

  return @(
    'proptest::proptest',
    'quickcheck::quickcheck',
    'rstest::rstest',
    'test_case::test_case',
    'parameterized::parameterized'
  ) -ccontains $Path
}

function Add-RustTestAliasEdge {
  param(
    [Parameter(Mandatory = $true)]$EdgesByKey,
    [Parameter(Mandatory = $true)][string]$Source,
    [Parameter(Mandatory = $true)][string]$Alias
  )

  $key = "$Source$([char]0)$Alias"
  if (-not $EdgesByKey.ContainsKey($key)) {
    $EdgesByKey.Add($key, [PSCustomObject]@{ Source = $Source; Alias = $Alias })
  }
}

function Get-RustTestAliases {
  param([Parameter(Mandatory = $true)][string]$LexicalMask)

  $aliases = [PSCustomObject]@{
    Attribute = New-RustGuardOrdinalDictionary
    Macro = New-RustGuardOrdinalDictionary
  }
  $edgesByKey = New-RustGuardOrdinalDictionary

  $simplePattern = '(?m)(?<![A-Za-z0-9_])use\s+([A-Za-z_][A-Za-z0-9_]*(?:\s*::\s*[A-Za-z_][A-Za-z0-9_]*)*)\s+as\s+([A-Za-z_][A-Za-z0-9_]*)\s*;'
  foreach ($match in [regex]::Matches($LexicalMask, $simplePattern)) {
    Add-RustTestAliasEdge `
      -EdgesByKey $edgesByKey `
      -Source ($match.Groups[1].Value -replace '\s', '') `
      -Alias $match.Groups[2].Value
  }

  $bracePattern = '(?ms)(?<![A-Za-z0-9_])use\s+([A-Za-z_][A-Za-z0-9_]*(?:\s*::\s*[A-Za-z_][A-Za-z0-9_]*)*)\s*::\s*\{([^{}]*)\}\s*;'
  foreach ($match in [regex]::Matches($LexicalMask, $bracePattern)) {
    $prefix = $match.Groups[1].Value -replace '\s', ''
    foreach ($item in @($match.Groups[2].Value -split ',')) {
      $aliasMatch = [regex]::Match($item, '^\s*([A-Za-z_][A-Za-z0-9_]*)\s+as\s+([A-Za-z_][A-Za-z0-9_]*)\s*$')
      if ($aliasMatch.Success) {
        Add-RustTestAliasEdge -EdgesByKey $edgesByKey -Source "$prefix::$($aliasMatch.Groups[1].Value)" -Alias $aliasMatch.Groups[2].Value
      }
    }
  }

  $rootBracePattern = '(?ms)(?<![A-Za-z0-9_])use\s*\{([^{}]*)\}\s*;'
  foreach ($match in [regex]::Matches($LexicalMask, $rootBracePattern)) {
    foreach ($item in @($match.Groups[1].Value -split ',')) {
      $aliasMatch = [regex]::Match($item, '^\s*([A-Za-z_][A-Za-z0-9_]*(?:\s*::\s*[A-Za-z_][A-Za-z0-9_]*)*)\s+as\s+([A-Za-z_][A-Za-z0-9_]*)\s*$')
      if ($aliasMatch.Success) {
        Add-RustTestAliasEdge `
          -EdgesByKey $edgesByKey `
          -Source ($aliasMatch.Groups[1].Value -replace '\s', '') `
          -Alias $aliasMatch.Groups[2].Value
      }
    }
  }

  $changed = $true
  while ($changed) {
    $changed = $false
    foreach ($key in (Get-RustGuardOrdinalSortedStrings -Values ([string[]]@($edgesByKey.Keys)))) {
      $edge = $edgesByKey[$key]
      $attributeSource = (Test-IsKnownTestAttributeImport -Path $edge.Source) -or
        $aliases.Attribute.ContainsKey($edge.Source)
      if ($attributeSource -and -not $aliases.Attribute.ContainsKey($edge.Alias)) {
        $aliases.Attribute.Add([string]$edge.Alias, $true)
        $changed = $true
      }

      $macroSource = (Test-IsKnownTestMacroImport -Path $edge.Source) -or
        $aliases.Macro.ContainsKey($edge.Source)
      if ($macroSource -and -not $aliases.Macro.ContainsKey($edge.Alias)) {
        $aliases.Macro.Add([string]$edge.Alias, $true)
        $changed = $true
      }
    }
  }
  return $aliases
}

function Test-IsTestAttributePath {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [AllowNull()]$AttributeAliases = $null
  )

  if ($null -ne $AttributeAliases -and $AttributeAliases.ContainsKey($Path)) {
    return $true
  }

  $segments = @($Path -split '::')
  $leaf = $segments[$segments.Count - 1]
  return @(
    'test',
    'rstest',
    'test_case',
    'wasm_bindgen_test',
    'parameterized',
    'quickcheck',
    'proptest'
  ) -ccontains $leaf
}

function Get-TestMarkerCountInAttribute {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Body,
    [AllowNull()]$AttributeAliases = $null
  )

  if (Test-IsTestAttributePath -Path $Path -AttributeAliases $AttributeAliases) {
    return 1
  }
  if ($Path -cne 'cfg_attr') {
    return 0
  }

  $payload = Get-CfgAttrPayload -AttributeBody $Body
  if ([string]::IsNullOrWhiteSpace($payload)) {
    return 0
  }

  $count = 0
  $condition = Get-CfgAttrCondition -AttributeBody $Body
  $conditionImpliesTest = -not [string]::IsNullOrWhiteSpace($condition) -and
    (Test-RustCfgExpressionImpliesTest -Expression $condition)
  $pathPattern = '(?<![A-Za-z0-9_])([A-Za-z_][A-Za-z0-9_]*(?:\s*::\s*[A-Za-z_][A-Za-z0-9_]*)*)'
  foreach ($match in [regex]::Matches($payload, $pathPattern)) {
    $candidate = $match.Groups[1].Value -replace '\s', ''
    if (Test-IsTestAttributePath -Path $candidate -AttributeAliases $AttributeAliases) {
      $count++
    }
  }
  if ($conditionImpliesTest -and
      [regex]::IsMatch($payload, '(?<![A-Za-z0-9_])path\s*=')) {
    $count++
  }
  return $count
}

function Get-RustAttributes {
  param(
    [Parameter(Mandatory = $true)][string]$LexicalMask,
    [AllowNull()]$AttributeAliases = $null
  )

  $attributes = @()
  $seen = @{}
  foreach ($match in [regex]::Matches($LexicalMask, '#\s*!?\s*\[')) {
    $open = $LexicalMask.IndexOf('[', $match.Index)
    if ($open -lt 0 -or $seen.ContainsKey($open)) {
      continue
    }
    $seen[$open] = $true
    $close = Find-MatchingRustDelimiter -Mask $LexicalMask -OpenIndex $open -Open '[' -Close ']'
    if ($close -lt 0) {
      continue
    }

    $body = $LexicalMask.Substring($open + 1, $close - $open - 1)
    $pathMatch = [regex]::Match($body, '^\s*([A-Za-z_][A-Za-z0-9_]*(?:\s*::\s*[A-Za-z_][A-Za-z0-9_]*)*)')
    if (-not $pathMatch.Success) {
      continue
    }
    $path = $pathMatch.Groups[1].Value -replace '\s', ''
    $isInner = $LexicalMask.Substring($match.Index, $open - $match.Index).Contains('!')
    $hasTestCondition = $path -ceq 'cfg' -and
      (Test-RustCfgAttributeImpliesTest -AttributeBody $body)
    $attributes += [PSCustomObject]@{
      Start = $match.Index
      Open = $open
      End = $close
      Path = $path
      IsInner = $isInner
      IsTestCondition = $hasTestCondition
      TestMarkers = Get-TestMarkerCountInAttribute -Path $path -Body $body -AttributeAliases $AttributeAliases
    }
  }
  return @($attributes | Sort-Object -Property Start)
}

function Test-IsSrcTestFile {
  param(
    [Parameter(Mandatory = $true)][System.IO.FileInfo]$File,
    [Parameter(Mandatory = $true)][string]$UnitRoot
  )

  $relative = $File.FullName.Substring($UnitRoot.Length).TrimStart([char[]]@('\', '/')) -replace '\\', '/'
  return $relative -match '^src/(tests|benches|examples)/' -or
    (Test-RustGuardExplicitSrcTestFileName -Name $File.Name)
}

function Test-IsNormalizedRepositoryPath {
  param([Parameter(Mandatory = $true)][string]$Path)

  return Test-RustGuardNormalizedRepositoryPath -Path $Path
}

function Sort-RowsByOrdinalPath {
  param([array]$Rows = @())

  $sorted = New-Object System.Collections.ArrayList
  foreach ($row in $Rows) {
    $low = 0
    $high = $sorted.Count
    while ($low -lt $high) {
      $middle = $low + [int][Math]::Floor(($high - $low) / 2.0)
      if ([string]::CompareOrdinal([string]$sorted[$middle].path, [string]$row.path) -lt 0) {
        $low = $middle + 1
      } else {
        $high = $middle
      }
    }
    [void]$sorted.Insert($low, $row)
  }
  return @($sorted)
}

function Remove-ValidExternalTestBridges {
  param(
    [Parameter(Mandatory = $true)][System.IO.FileInfo]$File,
    [Parameter(Mandatory = $true)][string]$UnitRoot,
    [Parameter(Mandatory = $true)][string]$Content
  )

  $bridgePattern = '(?m)^[ \t]*#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\][ \t]*\r?\n[ \t]*#\s*\[\s*path\s*=\s*"([^"\r\n]+)"\s*\][ \t]*\r?\n[ \t]*mod[ \t]+tests[ \t]*;[ \t]*(?=\r?$)'
  $testsRoot = Join-Path $UnitRoot 'tests'
  $unitTestsRoot = Join-Path $testsRoot 'unit'
  $lexicalMask = Get-RustLexicalMask -Content $Content
  return [regex]::Replace($Content, $bridgePattern, {
    param($match)

    $hashOffset = $match.Value.IndexOf('#')
    if ($hashOffset -lt 0 -or $lexicalMask[$match.Index + $hashOffset] -ne '#') {
      return $match.Value
    }

    $bridgePath = $match.Groups[1].Value
    if ([System.IO.Path]::IsPathRooted($bridgePath)) {
      return $match.Value
    }

    $candidate = [System.IO.Path]::GetFullPath((Join-Path $File.DirectoryName $bridgePath))
    if (-not (Test-Path -LiteralPath $candidate -PathType Leaf) -or
        -not (Test-Path -LiteralPath $unitTestsRoot -PathType Container)) {
      return $match.Value
    }
    if (Test-RustGuardPathContainsReparsePoint -RootPath $UnitRoot -TargetPath $candidate) {
      return $match.Value
    }

    $canonicalTarget = (Resolve-Path -LiteralPath $candidate).Path
    $canonicalUnitTestsRoot = (Resolve-Path -LiteralPath $unitTestsRoot).Path.TrimEnd([char[]]@('\', '/'))
    $unitTestsPrefix = $canonicalUnitTestsRoot + [System.IO.Path]::DirectorySeparatorChar
    if ($canonicalTarget.StartsWith($unitTestsPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
      return [regex]::Replace($match.Value, '[^\r\n]', ' ')
    }

    return $match.Value
  })
}

function Get-InlineTestModuleRanges {
  param(
    [Parameter(Mandatory = $true)][string]$Content,
    [Parameter(Mandatory = $true)][string]$LexicalMask,
    [array]$Attributes = @()
  )

  $rangesByBrace = @{}
  foreach ($attribute in ($Attributes | Where-Object { $_.IsTestCondition -and -not $_.IsInner })) {
    $cursor = Get-NextNonWhitespaceIndex -Text $LexicalMask -Start ($attribute.End + 1)

    while ($cursor -lt $LexicalMask.Length -and $LexicalMask[$cursor] -eq '#') {
      $attributeOpen = Get-NextNonWhitespaceIndex -Text $LexicalMask -Start ($cursor + 1)
      if ($attributeOpen -ge $LexicalMask.Length -or $LexicalMask[$attributeOpen] -ne '[') {
        break
      }
      $attributeClose = Find-MatchingRustDelimiter -Mask $LexicalMask -OpenIndex $attributeOpen -Open '[' -Close ']'
      if ($attributeClose -lt 0) {
        break
      }
      $cursor = Get-NextNonWhitespaceIndex -Text $LexicalMask -Start ($attributeClose + 1)
    }

    if (Test-RustKeywordAt -Text $LexicalMask -Index $cursor -Keyword 'pub') {
      $cursor = Get-NextNonWhitespaceIndex -Text $LexicalMask -Start ($cursor + 3)
      if ($cursor -lt $LexicalMask.Length -and $LexicalMask[$cursor] -eq '(') {
        $visibilityEnd = Find-MatchingRustDelimiter -Mask $LexicalMask -OpenIndex $cursor -Open '(' -Close ')'
        if ($visibilityEnd -lt 0) {
          continue
        }
        $cursor = Get-NextNonWhitespaceIndex -Text $LexicalMask -Start ($visibilityEnd + 1)
      }
    }

    if (-not (Test-RustKeywordAt -Text $LexicalMask -Index $cursor -Keyword 'mod')) {
      continue
    }
    $cursor = Get-NextNonWhitespaceIndex -Text $LexicalMask -Start ($cursor + 3)
    if ($cursor -ge $LexicalMask.Length) {
      continue
    }
    $nameMatch = [regex]::Match(
      $LexicalMask.Substring($cursor),
      '^(?:r#)?(?:[_\p{L}])(?:[_\p{L}\p{Nd}\p{Mn}\p{Mc}\p{Pc}]*)'
    )
    if (-not $nameMatch.Success) {
      continue
    }
    $cursor = Get-NextNonWhitespaceIndex -Text $LexicalMask -Start ($cursor + $nameMatch.Length)
    if ($cursor -ge $LexicalMask.Length -or $LexicalMask[$cursor] -ne '{') {
      continue
    }

    $end = Find-MatchingRustDelimiter -Mask $LexicalMask -OpenIndex $cursor -Open '{' -Close '}'
    if ($end -lt 0) {
      continue
    }
    $range = [PSCustomObject]@{
      Start = $attribute.Start
      End = $end
      OpenBrace = $cursor
      Text = $Content.Substring($attribute.Start, $end - $attribute.Start + 1)
    }
    if (-not $rangesByBrace.ContainsKey($cursor) -or $range.Start -lt $rangesByBrace[$cursor].Start) {
      $rangesByBrace[$cursor] = $range
    }
  }

  return @($rangesByBrace.Values | Sort-Object -Property Start)
}

function Test-IndexWithinRanges {
  param(
    [Parameter(Mandatory = $true)][int]$Index,
    [array]$Ranges = @()
  )

  foreach ($range in $Ranges) {
    if ($range.Start -le $Index -and $Index -le $range.End) {
      return $true
    }
    if ($range.Start -gt $Index) {
      break
    }
  }
  return $false
}

function Get-ExplicitTestMacroCount {
  param(
    [Parameter(Mandatory = $true)][string]$LexicalMask,
    [AllowNull()]$MacroAliases = $null
  )

  $macroPattern = '(?<![A-Za-z0-9_])(?:(?:[A-Za-z_][A-Za-z0-9_]*|r#[A-Za-z_][A-Za-z0-9_]*)\s*::\s*)*(?:proptest|quickcheck|rstest|test_case|parameterized)\s*!'
  $count = ([regex]::Matches($LexicalMask, $macroPattern)).Count
  if ($null -ne $MacroAliases) {
    foreach ($alias in $MacroAliases.Keys) {
      $pattern = '(?<![A-Za-z0-9_])' + [regex]::Escape([string]$alias) + '\s*!'
      $count += ([regex]::Matches($LexicalMask, $pattern)).Count
    }
  }
  return $count
}

function Get-RustSyntaxDebt {
  param([Parameter(Mandatory = $true)][string]$Content)

  $lexicalMask = Get-RustLexicalMask -Content $Content
  $aliases = Get-RustTestAliases -LexicalMask $lexicalMask
  $attributes = @(Get-RustAttributes -LexicalMask $lexicalMask -AttributeAliases $aliases.Attribute)
  $inlineRanges = @(Get-InlineTestModuleRanges -Content $Content -LexicalMask $lexicalMask -Attributes $attributes)

  $inlineLines = 0
  foreach ($range in $inlineRanges) {
    $inlineLines += Get-LineCountFromText -Content $range.Text
  }

  $testOnlyCfgItems = 0
  $testAttributes = Get-ExplicitTestMacroCount -LexicalMask $lexicalMask -MacroAliases $aliases.Macro
  foreach ($attribute in $attributes) {
    $testAttributes += [int]$attribute.TestMarkers
    if ($attribute.IsTestCondition -and -not (Test-IndexWithinRanges -Index $attribute.Start -Ranges $inlineRanges)) {
      $testOnlyCfgItems++
    }
  }

  return [PSCustomObject]@{
    InlineRanges = $inlineRanges
    inlineTestModules = $inlineRanges.Count
    inlineTestModuleLines = $inlineLines
    testAttributes = $testAttributes
    modTestsBlocks = ([regex]::Matches($lexicalMask, '\bmod\s+tests\s*\{')).Count
    testOnlyCfgItems = $testOnlyCfgItems
  }
}

function Get-TestDebtForFile {
  param(
    [Parameter(Mandatory = $true)][System.IO.FileInfo]$File,
    [Parameter(Mandatory = $true)][string]$UnitRoot
  )

  $relative = Get-NormalizedRelativePath -FullName $File.FullName
  $content = Remove-ValidExternalTestBridges -File $File -UnitRoot $UnitRoot -Content (Read-StrictUtf8Text -Path $File.FullName)
  $syntax = Get-RustSyntaxDebt -Content $content

  $srcTestFileLines = 0
  if (Test-IsSrcTestFile -File $File -UnitRoot $UnitRoot) {
    $srcTestFileLines = Get-LineCountFromText -Content $content
  }

  return [PSCustomObject]@{
    path = $relative
    inlineTestModules = $syntax.inlineTestModules
    inlineTestModuleLines = $syntax.inlineTestModuleLines
    testAttributes = $syntax.testAttributes
    modTestsBlocks = $syntax.modTestsBlocks
    testOnlyCfgItems = $syntax.testOnlyCfgItems
    srcTestFileLines = $srcTestFileLines
  }
}

function Get-CurrentTestDebt {
  $rows = @()
  foreach ($entry in @(Get-RustGuardFiles -RepoRoot $repoRoot -Mode TestLayout)) {
    $row = Get-TestDebtForFile -File $entry.File -UnitRoot $entry.UnitRoot
    $total = 0
    foreach ($field in $MetricFields) {
      $total += [int]$row.$field
    }
    if ($total -gt 0) {
      $rows += $row
    }
  }

  return @(Sort-RowsByOrdinalPath -Rows $rows)
}

function Read-TestLayoutBaseline {
  if (-not (Test-Path -LiteralPath $BaselinePath -PathType Leaf)) {
    throw "Rust test-layout baseline is missing: $BaselinePath"
  }

  return ConvertFrom-TestLayoutBaselineText -Content (Read-StrictUtf8Text -Path $BaselinePath) -Source $BaselinePath
}

function ConvertTo-RequiredInt {
  param(
    [Parameter(Mandatory = $true)]$Value,
    [Parameter(Mandatory = $true)][string]$Field,
    [Parameter(Mandatory = $true)][string]$Path
  )

  return ConvertTo-RustGuardCanonicalInt -Value $Value -Field $Field -Identity $Path -AllowZero
}

function Test-IsRustTestLayoutRepositoryPath {
  param([Parameter(Mandatory = $true)][string]$Path)

  if (-not (Test-RustGuardNormalizedRepositoryPath -Path $Path)) {
    return $false
  }
  if ($Path -ceq 'crates/evtx-patched/build.rs' -or
      $Path.StartsWith('crates/evtx-patched/src/', [System.StringComparison]::Ordinal)) {
    return $false
  }
  return $true
}

function ConvertFrom-TestLayoutBaselineText {
  param(
    [Parameter(Mandatory = $true)][string]$Content,
    [Parameter(Mandatory = $true)][string]$Source
  )

  $rows = @(ConvertFrom-RustGuardCsv -Content $Content -Header $BaselineHeader -Source $Source)
  $byPath = New-RustGuardOrdinalDictionary
  $previousPath = $null
  foreach ($entry in $rows) {
    foreach ($field in @('path') + $MetricFields) {
      if ($entry.PSObject.Properties.Name -notcontains $field) {
        throw "Rust test-layout baseline at $Source is missing required field '$field'"
      }
    }
    if ([string]::IsNullOrWhiteSpace([string]$entry.path)) {
      throw "Rust test-layout baseline at $Source contains an empty path"
    }
    if ($byPath.ContainsKey($entry.path)) {
      throw "Rust test-layout baseline at $Source contains a duplicate path: $($entry.path)"
    }
    if (-not (Test-IsRustTestLayoutRepositoryPath -Path ([string]$entry.path))) {
      throw "Excluded or invalid source must not appear in the Rust test-layout baseline at ${Source}: $($entry.path)"
    }
    if ($null -ne $previousPath -and [string]::CompareOrdinal($previousPath, $entry.path) -ge 0) {
      throw "Rust test-layout baseline at $Source is not in deterministic ordinal path order near: $($entry.path)"
    }
    $previousPath = $entry.path

    $metricTotal = 0
    foreach ($field in $MetricFields) {
      $value = ConvertTo-RequiredInt -Value $entry.$field -Field $field -Path $entry.path
      $entry.$field = $value
      $metricTotal += $value
    }
    if ($metricTotal -eq 0) {
      throw "Rust test-layout baseline row at $Source does not describe debt: $($entry.path)"
    }
    $byPath.Add([string]$entry.path, $entry)
  }

  $canonical = @((Write-TestLayoutBaselineCsv -Rows $rows)) -join "`n"
  Assert-RustGuardCanonicalCsvText -Content $Content -Canonical $canonical -Source $Source

  return [PSCustomObject]@{
    Rows = [object[]]$rows
    ByPath = $byPath
  }
}

function Write-TestLayoutBaselineCsv {
  param([Parameter(Mandatory = $true)][AllowEmptyCollection()][array]$Rows)

  Write-Output $BaselineHeader
  foreach ($row in (Sort-RowsByOrdinalPath -Rows $Rows)) {
    Write-Output ('{0},{1},{2},{3},{4},{5},{6}' -f
      (Format-RustGuardCsvField -Value ([string]$row.path)),
      [int]$row.inlineTestModules,
      [int]$row.inlineTestModuleLines,
      [int]$row.testAttributes,
      [int]$row.modTestsBlocks,
      [int]$row.testOnlyCfgItems,
      [int]$row.srcTestFileLines)
  }
}

function Get-RepositoryRelativeBaselinePath {
  param([Parameter(Mandatory = $true)][string]$Path)

  $fullPath = [System.IO.Path]::GetFullPath($Path)
  $rootPrefix = $repoRoot.TrimEnd([char[]]@('\', '/')) + [System.IO.Path]::DirectorySeparatorChar
  if (-not $fullPath.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Test-layout baseline transition files must remain inside the repository: $fullPath"
  }
  return Get-NormalizedRelativePath -FullName $fullPath
}

function Invoke-GitCapture {
  param([Parameter(Mandatory = $true)][string[]]$Arguments)

  $previousPreference = $ErrorActionPreference
  $exitCode = -1
  $output = @()
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

function Resolve-TestLayoutReferenceCommit {
  $candidate = $ReferenceRevision
  if ([string]::IsNullOrWhiteSpace($candidate)) {
    $candidate = [Environment]::GetEnvironmentVariable('RUST_TEST_LAYOUT_BASELINE_REFERENCE')
  }
  if ([string]::IsNullOrWhiteSpace($candidate)) {
    $candidate = 'HEAD'
  }

  $result = Invoke-GitCapture -Arguments @('-C', $repoRoot, 'rev-parse', '--verify', "${candidate}^{commit}")
  if ($result.ExitCode -ne 0 -or $result.Output.Count -ne 1 -or $result.Output[0] -notmatch '^[0-9a-fA-F]{40}$') {
    throw "Unable to resolve Rust test-layout baseline reference revision '$candidate': $($result.Output -join ' ')"
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

function Assert-TestLayoutBootstrapManifest {
  param(
    [Parameter(Mandatory = $true)][string]$ReferenceCommit,
    [AllowEmptyString()][string]$ExpectedSha256 = $TrustedBootstrapSha256,
    [string]$ManifestPath = $BootstrapManifestPath,
    [string]$BaselineFilePath = $BaselinePath
  )

  if ($ReferenceCommit -cne $BootstrapReferenceRevision) {
    throw "Rust test-layout bootstrap is only authorized against $BootstrapReferenceRevision, not $ReferenceCommit"
  }
  if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) {
    throw "Rust test-layout baseline does not exist at reference $ReferenceCommit and requires the explicit one-time bootstrap manifest: $ManifestPath"
  }
  $content = Read-StrictUtf8Text -Path $ManifestPath
  $header = 'referenceRevision,baselineSha256'
  $rows = @(ConvertFrom-RustGuardCsv -Content $content -Header $header -Source $ManifestPath)
  if ($rows.Count -ne 1) {
    throw 'Rust test-layout bootstrap manifest must contain exactly one authorization row'
  }
  $row = $rows[0]
  foreach ($field in @('referenceRevision', 'baselineSha256')) {
    if ($row.PSObject.Properties.Name -notcontains $field -or [string]::IsNullOrWhiteSpace([string]$row.$field)) {
      throw "Rust test-layout bootstrap manifest contains an empty required field '$field'"
    }
  }
  if ($row.referenceRevision -notmatch '^[0-9a-f]{40}$' -or $row.referenceRevision -cne $ReferenceCommit) {
    throw "Rust test-layout bootstrap manifest reference does not match resolved reference commit: expected $ReferenceCommit, found $($row.referenceRevision)"
  }
  if ($row.baselineSha256 -notmatch '^[0-9a-f]{64}$') {
    throw 'Rust test-layout bootstrap manifest contains an invalid baselineSha256'
  }

  $canonical = "$header`n$($row.referenceRevision),$($row.baselineSha256)"
  Assert-RustGuardCanonicalCsvText -Content $content -Canonical $canonical -Source $ManifestPath

  $actualHash = Get-Sha256Hex -Bytes ([System.IO.File]::ReadAllBytes($BaselineFilePath))
  if ($actualHash -cne $row.baselineSha256) {
    throw "Rust test-layout bootstrap manifest does not authorize the current baseline bytes: expected $($row.baselineSha256), found $actualHash"
  }
  Assert-RustGuardTrustedBootstrapSha256 `
    -GuardName 'Rust test-layout' `
    -ExpectedSha256 $ExpectedSha256 `
    -ManifestSha256 ([string]$row.baselineSha256) `
    -ActualSha256 $actualHash
}

function Get-TestLayoutBaselineTransitionFailures {
  param(
    [Parameter(Mandatory = $true)]$CurrentBaseline,
    [Parameter(Mandatory = $true)]$ReferenceBaseline
  )

  $failures = @()
  foreach ($entry in $CurrentBaseline.Rows) {
    if (-not $ReferenceBaseline.ByPath.ContainsKey($entry.path)) {
      $failures += "baseline transition added path: $($entry.path)"
      continue
    }
    $referenceEntry = $ReferenceBaseline.ByPath[$entry.path]
    if ($entry.path -cne $referenceEntry.path) {
      $failures += "baseline transition changed path casing: $($referenceEntry.path) -> $($entry.path)"
      continue
    }
    foreach ($field in $MetricFields) {
      $referenceValue = [int]$referenceEntry.$field
      $currentValue = [int]$entry.$field
      if ($currentValue -gt $referenceValue) {
        $failures += "baseline transition increased ${field}: $($entry.path) from $referenceValue to $currentValue"
      }
    }
  }
  return $failures
}

function Assert-TestLayoutBaselineTransition {
  param(
    [Parameter(Mandatory = $true)]$CurrentBaseline,
    [Parameter(Mandatory = $true)][string]$ReferenceCommit
  )

  $baselineRepoPath = Get-RepositoryRelativeBaselinePath -Path $BaselinePath
  $referenceFile = Get-GitFileAtRevision -Revision $ReferenceCommit -RepositoryPath $baselineRepoPath
  if (-not $referenceFile.Exists) {
    Assert-TestLayoutBootstrapManifest -ReferenceCommit $ReferenceCommit
    Write-Host "Rust test-layout baseline transition: explicit bootstrap authorized against $ReferenceCommit"
    return
  }

  $referenceBaseline = ConvertFrom-TestLayoutBaselineText -Content $referenceFile.Content -Source "$ReferenceCommit`:$baselineRepoPath"
  $transitionFailures = @(
    Get-TestLayoutBaselineTransitionFailures -CurrentBaseline $CurrentBaseline -ReferenceBaseline $referenceBaseline
  )
  if ($transitionFailures.Count -gt 0) {
    throw "Rust test-layout baseline transition rejected against ${ReferenceCommit}:`n$($transitionFailures -join "`n")"
  }

  Write-Host ('Rust test-layout baseline transition passed: reference={0}, reference rows={1}, current rows={2}; only per-field decreases and path deletions allowed' -f $ReferenceCommit, $referenceBaseline.Rows.Count, $CurrentBaseline.Rows.Count)
}

function Assert-SelfTestEqual {
  param(
    [Parameter(Mandatory = $true)][string]$Name,
    [Parameter(Mandatory = $true)]$Actual,
    [Parameter(Mandatory = $true)]$Expected
  )

  if ($Actual -ne $Expected) {
    throw "Rust test-layout self-test failed for ${Name}: expected=$Expected actual=$Actual"
  }
}

function Invoke-RustTestLayoutSelfTest {
  Invoke-RustGuardWorkspaceDiscoverySelfTest -Encoding $StrictUtf8 -CodeTargetAssertion {
    param($File, $Content, $HelperFile, $HelperContent)

    $targetDebt = Get-RustSyntaxDebt -Content $Content
    $helperDebt = Get-RustSyntaxDebt -Content $HelperContent
    if ($targetDebt.testAttributes -ne 1 -or $helperDebt.testAttributes -ne 1) {
      throw 'Test-layout guard did not scan test attributes in the non-.rs Cargo target and recursive helper module'
    }
  }

  $fixture = @'
const NORMAL: &str = "} #[test] mod tests { proptest!";
const ESCAPED: &str = "\"} #[test]";
const BYTE: &[u8] = b"} { #[rstest]";
const C_STRING: &CStr = c"} { #[test_case]";
const RAW: &str = r###"} #[test] mod tests {"###;
const RAW_BYTE: &[u8] = br##"} proptest! {"##;
const RAW_C: &CStr = cr#"} mod tests {"#;
const CHARACTER: char = '}';
const ESCAPED_CHARACTER: char = '\'';
const BYTE_CHARACTER: u8 = b'}';
const HEX_BYTE_CHARACTER: u8 = b'\x7d';
const UNICODE_CHARACTER: char = '\u{007d}';
// } #[test] mod tests { quickcheck!
/* outer } #[test] /* nested { proptest! */ still } */
#[cfg(all(test, feature = "fixture"))]
mod tests {
    const INNER: &str = "}";
    const INNER_RAW: &str = r#"}"#;
    const INNER_CHARACTER: char = '}';
    // }
    /* } /* { */ */
    #[tokio::test]
    async fn async_case() {}
    proptest! {
        fn property_case(value in 0u8..=255) {
            prop_assert!(value <= 255);
        }
    }
}
#[cfg_attr(any(test), path = "../tests/unit/conditional.rs")]
mod conditional;
#[cfg_attr(test, async_std::test)]
async fn conditional_async_case() {}
#[rstest]
fn parameter_case() {}
#[test_case(1)]
fn generated_case() {}
quickcheck! {
    fn quick_property(value: u8) -> bool { value <= 255 }
}
use tokio::test as audit_case;
#[audit_case]
async fn aliased_async_case() {}
use proptest::proptest as property_cases;
property_cases! { fn aliased_property(value in 0u8..=1) { prop_assert!(value <= 1); } }
use tokio::{test as brace_audit_case, spawn as ordinary_alias};
#[brace_audit_case]
async fn brace_aliased_async_case() {}
use proptest::{proptest as brace_property_cases};
brace_property_cases! { fn brace_aliased_property(value in 0u8..=1) { prop_assert!(value <= 1); } }
ordinary_alias!();
use tokio::test as first_case;
use first_case as second_case;
#[second_case]
async fn twice_aliased_case() {}
use tokio::{test as brace_first_case};
use {brace_first_case as brace_second_case};
#[brace_second_case]
async fn twice_brace_aliased_case() {}
use std::fmt as ordinary_first;
use ordinary_first as ordinary_second;
#[ordinary_second]
fn ordinary_alias_is_not_a_test() {}
'@

  $mask = Get-RustLexicalMask -Content $fixture
  Assert-SelfTestEqual -Name 'lexical mask length' -Actual $mask.Length -Expected $fixture.Length
  for ($index = 0; $index -lt $fixture.Length; $index++) {
    $sourceIsNewline = $fixture[$index] -eq "`r" -or $fixture[$index] -eq "`n"
    $maskIsNewline = $mask[$index] -eq "`r" -or $mask[$index] -eq "`n"
    if ($sourceIsNewline -ne $maskIsNewline -or ($sourceIsNewline -and $fixture[$index] -ne $mask[$index])) {
      throw "Rust lexical mask did not preserve newline at index $index"
    }
  }

  $syntax = Get-RustSyntaxDebt -Content $fixture
  $cfgStart = $fixture.IndexOf('#[cfg(all(test')
  $afterModule = $fixture.IndexOf('#[cfg_attr(any(test)', $cfgStart)
  $expectedModuleText = $fixture.Substring($cfgStart, $afterModule - $cfgStart).TrimEnd([char[]]@("`r", "`n"))
  $expectedModuleLines = Get-LineCountFromText -Content $expectedModuleText
  Assert-SelfTestEqual -Name 'inline module count' -Actual $syntax.inlineTestModules -Expected 1
  Assert-SelfTestEqual -Name 'inline module lines' -Actual $syntax.inlineTestModuleLines -Expected $expectedModuleLines
  Assert-SelfTestEqual -Name 'mod tests block count' -Actual $syntax.modTestsBlocks -Expected 1
  Assert-SelfTestEqual -Name 'test-conditioned item count' -Actual $syntax.testOnlyCfgItems -Expected 0
  Assert-SelfTestEqual -Name 'test attribute and macro count' -Actual $syntax.testAttributes -Expected 13
  if ($syntax.InlineRanges[0].Text -notmatch 'property_case' -or
      $syntax.InlineRanges[0].Text -match 'conditional_async_case') {
    throw 'Rust lexical mask resolved an incorrect inline test module range'
  }

  $innerSyntax = Get-RustSyntaxDebt -Content "#![cfg_attr(test, allow(dead_code))]`nmod production {}`n"
  Assert-SelfTestEqual -Name 'inner cfg_attr is not attached to next module' -Actual $innerSyntax.inlineTestModules -Expected 0
  Assert-SelfTestEqual -Name 'non-test cfg_attr payload is not test debt' -Actual $innerSyntax.testOnlyCfgItems -Expected 0

  $negativeCfg = Get-RustSyntaxDebt -Content @'
#[cfg(all(not(test), windows))]
fn production_all_not_test() {}
#[cfg(any(not(test), windows))]
fn production_any_not_test() {}
'@
  Assert-SelfTestEqual -Name 'nested negative cfg is not test-only' -Actual $negativeCfg.testOnlyCfgItems -Expected 0

  $positiveCfg = Get-RustSyntaxDebt -Content "#[cfg(all(test, windows))]`nmod positive_tests {}`n"
  Assert-SelfTestEqual -Name 'provably test-only cfg module' -Actual $positiveCfg.inlineTestModules -Expected 1

  $referenceText = @"
$BaselineHeader
crates/sample/src/alpha.rs,2,100,3,1,1,0
crates/sample/src/beta.rs,1,50,1,1,0,0
"@.Trim()
  $decreasedText = @"
$BaselineHeader
crates/sample/src/alpha.rs,1,90,2,1,0,0
"@.Trim()
  $addedText = @"
$BaselineHeader
crates/sample/src/alpha.rs,1,90,2,1,0,0
crates/sample/src/gamma.rs,1,10,1,1,0,0
"@.Trim()
  $increasedText = @"
$BaselineHeader
crates/sample/src/alpha.rs,2,101,3,1,1,0
"@.Trim()
  $referenceBaseline = ConvertFrom-TestLayoutBaselineText -Content $referenceText -Source 'self-test reference'
  $decreasedBaseline = ConvertFrom-TestLayoutBaselineText -Content $decreasedText -Source 'self-test decreased'
  if (@(
      Get-TestLayoutBaselineTransitionFailures -CurrentBaseline $decreasedBaseline -ReferenceBaseline $referenceBaseline
    ).Count -ne 0) {
    throw 'Rust test-layout baseline transition rejected an allowed per-field decrease/path deletion'
  }
  foreach ($rejectedBaseline in @(
    (ConvertFrom-TestLayoutBaselineText -Content $addedText -Source 'self-test added'),
    (ConvertFrom-TestLayoutBaselineText -Content $increasedText -Source 'self-test increased')
  )) {
    if (@(
        Get-TestLayoutBaselineTransitionFailures -CurrentBaseline $rejectedBaseline -ReferenceBaseline $referenceBaseline
      ).Count -eq 0) {
      throw 'Rust test-layout baseline transition accepted an added path or increased metric'
    }
  }

  $emptyBaseline = ConvertFrom-TestLayoutBaselineText -Content $BaselineHeader -Source 'self-test empty'
  if (@($emptyBaseline.Rows).Count -ne 0 -or
      @(Get-TestLayoutBaselineTransitionFailures -CurrentBaseline $emptyBaseline -ReferenceBaseline $referenceBaseline).Count -ne 0) {
    throw 'Rust test-layout baseline did not accept a header-only zero-debt transition'
  }
  if ((@((Write-TestLayoutBaselineCsv -Rows @())) -join "`n") -cne $BaselineHeader) {
    throw 'Rust test-layout generator did not emit a header-only zero-debt baseline'
  }

  foreach ($invalidCase in @(
    "$BaselineHeader,extra`ncrates/sample/src/alpha.rs,1,10,1,1,0,0,hidden",
    "$BaselineHeader`ncrates/sample/src/alpha.rs,+1,10,1,1,0,0",
    "$BaselineHeader`ncrates/sample/src/alpha.rs,01,10,1,1,0,0"
  )) {
    $rejected = $false
    try {
      [void](ConvertFrom-TestLayoutBaselineText -Content $invalidCase -Source 'self-test strict CSV')
    } catch {
      $rejected = $true
    }
    if (-not $rejected) {
      throw 'Rust test-layout baseline accepted an extra column or non-canonical integer'
    }
  }

  $caseChangedText = "$BaselineHeader`ncrates/Sample/src/alpha.rs,1,90,2,1,0,0"
  $caseChangedBaseline = ConvertFrom-TestLayoutBaselineText -Content $caseChangedText -Source 'self-test case change'
  if (@(Get-TestLayoutBaselineTransitionFailures -CurrentBaseline $caseChangedBaseline -ReferenceBaseline $referenceBaseline).Count -eq 0) {
    throw 'Rust test-layout baseline transition accepted a case-only identity change'
  }

  $tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('meow-test-layout-' + [guid]::NewGuid().ToString('N'))
  try {
    $unitRoot = Join-Path $tempRoot 'crate'
    $sourceRoot = Join-Path $unitRoot 'src'
    $unitTestsRoot = Join-Path $unitRoot 'tests/unit'
    [void][System.IO.Directory]::CreateDirectory($sourceRoot)
    [void][System.IO.Directory]::CreateDirectory($unitTestsRoot)
    $sourcePath = Join-Path $sourceRoot 'lib.rs'
    $validTarget = Join-Path $unitTestsRoot 'valid.rs'
    $directTarget = Join-Path $unitRoot 'tests/direct.rs'
    [System.IO.File]::WriteAllText($sourcePath, '', $StrictUtf8)
    [System.IO.File]::WriteAllText($validTarget, "#[test]`nfn valid() {}`n", $StrictUtf8)
    [System.IO.File]::WriteAllText($directTarget, "#[test]`nfn direct() {}`n", $StrictUtf8)
    $sourceFile = Get-Item -LiteralPath $sourcePath

    $validBridge = "#[cfg(test)]`n#[path = `"../tests/unit/valid.rs`"]`nmod tests;`n"
    $validResult = Remove-ValidExternalTestBridges -File $sourceFile -UnitRoot $unitRoot -Content $validBridge
    $validSyntax = Get-RustSyntaxDebt -Content $validResult
    Assert-SelfTestEqual -Name 'valid unit bridge debt' -Actual (
      $validSyntax.inlineTestModules + $validSyntax.testAttributes +
      $validSyntax.modTestsBlocks + $validSyntax.testOnlyCfgItems
    ) -Expected 0

    $directBridge = "#[cfg(test)]`n#[path = `"../tests/direct.rs`"]`nmod tests;`n"
    $directResult = Remove-ValidExternalTestBridges -File $sourceFile -UnitRoot $unitRoot -Content $directBridge
    Assert-SelfTestEqual -Name 'direct integration bridge preserved' -Actual $directResult -Expected $directBridge
    Assert-SelfTestEqual -Name 'direct integration bridge debt' -Actual (
      (Get-RustSyntaxDebt -Content $directResult).testOnlyCfgItems
    ) -Expected 1

    $traversalBridge = "#[cfg(test)]`n#[path = `"../tests/unit/../direct.rs`"]`nmod tests;`n"
    $traversalResult = Remove-ValidExternalTestBridges -File $sourceFile -UnitRoot $unitRoot -Content $traversalBridge
    Assert-SelfTestEqual -Name 'canonical traversal bridge preserved' -Actual $traversalResult -Expected $traversalBridge

    $equivalentCfgBridge = "#[cfg(any(test))]`n#[path = `"../tests/unit/valid.rs`"]`nmod tests;`n"
    $equivalentCfgResult = Remove-ValidExternalTestBridges -File $sourceFile -UnitRoot $unitRoot -Content $equivalentCfgBridge
    Assert-SelfTestEqual -Name 'non-exact cfg bridge preserved' -Actual $equivalentCfgResult -Expected $equivalentCfgBridge
    Assert-SelfTestEqual -Name 'non-exact cfg bridge debt' -Actual (
      (Get-RustSyntaxDebt -Content $equivalentCfgResult).testOnlyCfgItems
    ) -Expected 1

    $commentedBridge = "/*`n#[cfg(test)]`n#[path = `"../tests/unit/valid.rs`"]`nmod tests;`n*/`n"
    $commentedResult = Remove-ValidExternalTestBridges -File $sourceFile -UnitRoot $unitRoot -Content $commentedBridge
    Assert-SelfTestEqual -Name 'commented bridge preserved' -Actual $commentedResult -Expected $commentedBridge
    Assert-SelfTestEqual -Name 'commented bridge ignored' -Actual (
      (Get-RustSyntaxDebt -Content $commentedResult).testOnlyCfgItems
    ) -Expected 0

    $missingPath = Join-Path $unitRoot 'tests/unit/missing/target.rs'
    if (-not (Test-RustGuardPathContainsReparsePoint -RootPath $unitRoot -TargetPath $missingPath)) {
      throw 'Reparse-point validator did not fail closed for an unresolved path component'
    }

    $junctionPath = Join-Path $unitTestsRoot 'escape'
    $junctionCreated = $false
    try {
      [void](New-Item -ItemType Junction -Path $junctionPath -Target (Join-Path $unitRoot 'tests') -ErrorAction Stop)
      $junctionCreated = $true
    } catch {
      $junctionCreated = $false
    }
    if ($junctionCreated) {
      $junctionTarget = Join-Path $junctionPath 'direct.rs'
      if (-not (Test-RustGuardPathContainsReparsePoint -RootPath $unitRoot -TargetPath $junctionTarget)) {
        throw 'Reparse-point validator accepted a junction component'
      }
      $junctionBridge = "#[cfg(test)]`n#[path = `"../tests/unit/escape/direct.rs`"]`nmod tests;`n"
      $junctionResult = Remove-ValidExternalTestBridges -File $sourceFile -UnitRoot $unitRoot -Content $junctionBridge
      Assert-SelfTestEqual -Name 'junction escape bridge preserved' -Actual $junctionResult -Expected $junctionBridge
      Assert-SelfTestEqual -Name 'junction escape bridge debt' -Actual (
        (Get-RustSyntaxDebt -Content $junctionResult).testOnlyCfgItems
      ) -Expected 1
    }

    $bootstrapBaselinePath = Join-Path $tempRoot 'bootstrap-baseline.csv'
    $bootstrapManifestPath = Join-Path $tempRoot 'bootstrap-manifest.csv'
    [System.IO.File]::WriteAllText($bootstrapBaselinePath, $decreasedText + "`n", $StrictUtf8)
    $bootstrapHash = Get-Sha256Hex -Bytes ([System.IO.File]::ReadAllBytes($bootstrapBaselinePath))
    [System.IO.File]::WriteAllText(
      $bootstrapManifestPath,
      "referenceRevision,baselineSha256`n$BootstrapReferenceRevision,$bootstrapHash`n",
      $StrictUtf8
    )
    Assert-TestLayoutBootstrapManifest `
      -ReferenceCommit $BootstrapReferenceRevision `
      -ExpectedSha256 $bootstrapHash `
      -ManifestPath $bootstrapManifestPath `
      -BaselineFilePath $bootstrapBaselinePath

    $wrongReferenceRejected = $false
    try {
      Assert-TestLayoutBootstrapManifest `
        -ReferenceCommit ('1' * 40) `
        -ExpectedSha256 $bootstrapHash `
        -ManifestPath $bootstrapManifestPath `
        -BaselineFilePath $bootstrapBaselinePath
    } catch {
      $wrongReferenceRejected = $_.Exception.Message -match 'only authorized against'
    }
    if (-not $wrongReferenceRejected) {
      throw 'Rust test-layout bootstrap accepted a non-bootstrap reference revision'
    }

    [System.IO.File]::WriteAllText(
      $bootstrapManifestPath,
      "referenceRevision,baselineSha256`n$BootstrapReferenceRevision,$('0' * 64)`n",
      $StrictUtf8
    )
    $bootstrapMismatch = $null
    try {
      Assert-TestLayoutBootstrapManifest `
        -ReferenceCommit $BootstrapReferenceRevision `
        -ExpectedSha256 $bootstrapHash `
        -ManifestPath $bootstrapManifestPath `
        -BaselineFilePath $bootstrapBaselinePath
    } catch {
      $bootstrapMismatch = $_.Exception.Message
    }
    if ([string]::IsNullOrWhiteSpace($bootstrapMismatch) -or
        $bootstrapMismatch -notmatch 'does not authorize the current baseline bytes') {
      throw 'Rust test-layout bootstrap accepted a baseline SHA-256 mismatch'
    }
    [System.IO.File]::WriteAllText(
      $bootstrapManifestPath,
      "referenceRevision,baselineSha256`n$BootstrapReferenceRevision,$bootstrapHash`n",
      $StrictUtf8
    )
    $untrustedBootstrapRejected = $false
    try {
      Assert-TestLayoutBootstrapManifest `
        -ReferenceCommit $BootstrapReferenceRevision `
        -ExpectedSha256 ('0' * 64) `
        -ManifestPath $bootstrapManifestPath `
        -BaselineFilePath $bootstrapBaselinePath
    } catch {
      $untrustedBootstrapRejected = $_.Exception.Message -match 'trusted SHA-256'
    }
    if (-not $untrustedBootstrapRejected) {
      throw 'Rust test-layout bootstrap accepted a PR-controlled manifest without trusted authorization'
    }
  } finally {
    if (Test-Path -LiteralPath $tempRoot) {
      Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
  }

  Write-Host 'Rust test-layout self-test passed: trusted bootstrap, case-safe physical identity, taskkill/job/snapshot tree cleanup, normal-process isolation, non-.rs target recursion, path/include rejection, fixed-point aliases, cfg, bridges, CSV, and transitions covered'
}

if ($SelfTest) {
  Invoke-RustTestLayoutSelfTest
  return
}

$current = @(Get-CurrentTestDebt)

if ($GenerateBaseline) {
  Write-TestLayoutBaselineCsv -Rows $current
  return
}

$currentByPath = New-RustGuardOrdinalDictionary
foreach ($row in $current) {
  if ($currentByPath.ContainsKey($row.path)) {
    throw "Rust test-layout scan produced a duplicate path: $($row.path)"
  }
  $currentByPath.Add([string]$row.path, $row)
}

$baselineDocument = Read-TestLayoutBaseline
$baseline = @($baselineDocument.Rows)
$baselineByPath = $baselineDocument.ByPath
foreach ($entry in $baseline) {
  if (-not $currentByPath.ContainsKey($entry.path)) {
    throw "Rust test-layout baseline is stale because the source test debt was removed: $($entry.path)"
  }
  if ($entry.path -cne $currentByPath[$entry.path].path) {
    throw "Rust test-layout baseline path casing does not match the current Windows path: $($entry.path)"
  }
}

$referenceCommit = Resolve-TestLayoutReferenceCommit
Assert-TestLayoutBaselineTransition -CurrentBaseline $baselineDocument -ReferenceCommit $referenceCommit

$failures = @()
foreach ($row in $current) {
  if (-not $baselineByPath.ContainsKey($row.path)) {
    $failures += 'new src test-layout debt: {0}' -f $row.path
    continue
  }

  $baselineEntry = $baselineByPath[$row.path]
  foreach ($field in $MetricFields) {
    $baselineValue = ConvertTo-RequiredInt -Value $baselineEntry.$field -Field $field -Path $row.path
    $currentValue = [int]$row.$field
    if ($currentValue -gt $baselineValue) {
      $failures += 'increased src test-layout debt: {0} {1} grew from {2} to {3}' -f $row.path, $field, $baselineValue, $currentValue
    }
  }
}

if ($failures.Count -gt 0) {
  Write-Error "Rust test layout guard failed:`n$($failures -join "`n")"
}

$inlineModuleTotal = 0
$testAttributeTotal = 0
$srcTestFileLineTotal = 0
foreach ($row in $current) {
  $inlineModuleTotal += [int]$row.inlineTestModules
  $testAttributeTotal += [int]$row.testAttributes
  $srcTestFileLineTotal += [int]$row.srcTestFileLines
}

Write-Host ('Rust test layout guard passed: current files={0}, baseline files={1}, inline modules={2}, test attributes={3}, src test-file lines={4}' -f $current.Count, $baseline.Count, $inlineModuleTotal, $testAttributeTotal, $srcTestFileLineTotal)
