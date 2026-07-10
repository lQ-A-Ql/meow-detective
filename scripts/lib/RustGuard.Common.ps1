# Requires -Version 5.1

Set-StrictMode -Version Latest

$script:RustGuardMetadataCache = @{}

function New-RustGuardOrdinalDictionary {
  return [System.Collections.Generic.Dictionary[string,object]]::new(
    [System.StringComparer]::Ordinal
  )
}

function Get-RustGuardFileSystemStringComparison {
  if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) {
    return [System.StringComparison]::OrdinalIgnoreCase
  }
  return [System.StringComparison]::Ordinal
}

function New-RustGuardFileIdentityDictionary {
  $comparer = if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) {
    [System.StringComparer]::OrdinalIgnoreCase
  } else {
    [System.StringComparer]::Ordinal
  }
  return [System.Collections.Generic.Dictionary[string,object]]::new($comparer)
}

function Get-RustGuardOrdinalSortedStrings {
  param([AllowEmptyCollection()][string[]]$Values = @())

  $copy = [string[]]@($Values)
  [System.Array]::Sort($copy, [System.StringComparer]::Ordinal)
  return $copy
}

function Get-RustGuardRepositoryRelativePath {
  param(
    [Parameter(Mandatory = $true)][string]$RepoRoot,
    [Parameter(Mandatory = $true)][string]$FullName
  )

  $root = [System.IO.Path]::GetFullPath($RepoRoot).TrimEnd([char[]]@('\', '/'))
  $full = [System.IO.Path]::GetFullPath($FullName)
  $prefix = $root + [System.IO.Path]::DirectorySeparatorChar
  if (-not $full.StartsWith($prefix, (Get-RustGuardFileSystemStringComparison))) {
    throw "Rust guard path is outside the repository: $full"
  }
  return $full.Substring($prefix.Length) -replace '\\', '/'
}

function Test-RustGuardNormalizedRepositoryPath {
  param([Parameter(Mandatory = $true)][string]$Path)

  if ([string]::IsNullOrWhiteSpace($Path) -or
      [System.IO.Path]::IsPathRooted($Path) -or
      $Path -match '\\' -or
      $Path -match '(^/|/$|//)' -or
      $Path -match '(^|/)\.\.?(/|$)' -or
      $Path -match '[\r\n]') {
    return $false
  }
  return $true
}

function Test-RustGuardExplicitSrcTestFileName {
  param([Parameter(Mandatory = $true)][string]$Name)

  return $Name -match '(^tests\.rs$|_tests\.rs$|_test\.rs$|\.test\.rs$|\.spec\.rs$|^test_utils\.rs$|^test_helpers\.rs$)'
}

function Test-RustGuardProductionRepositoryPath {
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

if (-not ('Stage0.RustGuardLexicalMasker' -as [type])) {
  Add-Type -Language CSharp -TypeDefinition @'
using System;

namespace Stage0
{
    public static class RustGuardLexicalMasker
    {
        public static string Mask(string source)
        {
            if (source == null) throw new ArgumentNullException("source");
            char[] masked = source.ToCharArray();
            int index = 0;
            while (index < source.Length)
            {
                int end;
                if (index + 1 < source.Length && source[index] == '/' && source[index + 1] == '/')
                {
                    end = index + 2;
                    while (end < source.Length && source[end] != '\r' && source[end] != '\n') end++;
                    Blank(masked, index, end);
                    index = end;
                    continue;
                }
                if (index + 1 < source.Length && source[index] == '/' && source[index + 1] == '*')
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
                else cursor++;
            }
            return cursor;
        }

        private static bool TryFindRawStringEnd(string source, int start, out int end)
        {
            end = start;
            if (!IsTokenBoundary(source, start)) return false;
            int cursor;
            if (source[start] == 'r') cursor = start + 1;
            else if ((source[start] == 'b' || source[start] == 'c') &&
                     start + 1 < source.Length && source[start + 1] == 'r') cursor = start + 2;
            else return false;

            int hashes = 0;
            while (cursor < source.Length && source[cursor] == '#') { hashes++; cursor++; }
            if (cursor >= source.Length || source[cursor] != '"') return false;
            cursor++;
            while (cursor < source.Length)
            {
                if (source[cursor] != '"') { cursor++; continue; }
                int suffix = cursor + 1;
                int seen = 0;
                while (seen < hashes && suffix < source.Length && source[suffix] == '#')
                {
                    suffix++;
                    seen++;
                }
                if (seen == hashes) { end = suffix; return true; }
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
                if (!escaped && current == '"') return cursor + 1;
                if (!escaped && current == '\\') escaped = true;
                else escaped = false;
                cursor++;
            }
            return source.Length;
        }

        private static bool TryFindCharacterEnd(string source, int quote, out int end)
        {
            end = quote;
            int cursor = quote + 1;
            if (cursor >= source.Length || source[cursor] == '\r' || source[cursor] == '\n') return false;
            if (source[cursor] == '\\')
            {
                cursor++;
                if (cursor >= source.Length) return false;
                if (source[cursor] == 'u' && cursor + 1 < source.Length && source[cursor + 1] == '{')
                {
                    cursor += 2;
                    while (cursor < source.Length && source[cursor] != '}' &&
                           source[cursor] != '\r' && source[cursor] != '\n') cursor++;
                    if (cursor >= source.Length || source[cursor] != '}') return false;
                    cursor++;
                }
                else if (source[cursor] == 'x')
                {
                    cursor += 3;
                    if (cursor > source.Length) return false;
                }
                else cursor++;
            }
            else if (Char.IsHighSurrogate(source[cursor]) && cursor + 1 < source.Length &&
                     Char.IsLowSurrogate(source[cursor + 1])) cursor += 2;
            else cursor++;
            if (cursor < source.Length && source[cursor] == '\'')
            {
                end = cursor + 1;
                return true;
            }
            return false;
        }

        private static bool IsTokenBoundary(string source, int index)
        {
            return index == 0 || !(source[index - 1] == '_' || Char.IsLetterOrDigit(source[index - 1]));
        }

        private static void Blank(char[] masked, int start, int end)
        {
            int limit = Math.Min(end, masked.Length);
            for (int index = start; index < limit; index++)
            {
                if (masked[index] != '\r' && masked[index] != '\n') masked[index] = ' ';
            }
        }
    }
}
'@
}

if (-not ('Stage0.RustGuardWindowsJob' -as [type])) {
  Add-Type -Language CSharp -TypeDefinition @'
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;

namespace Stage0
{
    public sealed class RustGuardWindowsJob : IDisposable
    {
        private const UInt32 JobObjectExtendedLimitInformation = 9;
        private const UInt32 JobObjectLimitKillOnJobClose = 0x00002000;
        private IntPtr handle;

        private RustGuardWindowsJob(IntPtr handle)
        {
            this.handle = handle;
        }

        public static RustGuardWindowsJob TryAssign(Process process)
        {
            if (process == null) throw new ArgumentNullException("process");
            IntPtr job = CreateJobObject(IntPtr.Zero, null);
            if (job == IntPtr.Zero) return null;
            try
            {
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits =
                    new JOBOBJECT_EXTENDED_LIMIT_INFORMATION();
                limits.BasicLimitInformation.LimitFlags = JobObjectLimitKillOnJobClose;
                UInt32 size = (UInt32)Marshal.SizeOf(typeof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION));
                if (!SetInformationJobObject(
                        job,
                        JobObjectExtendedLimitInformation,
                        ref limits,
                        size))
                {
                    return null;
                }
                if (!AssignProcessToJobObject(job, process.Handle)) return null;
                RustGuardWindowsJob assigned = new RustGuardWindowsJob(job);
                job = IntPtr.Zero;
                return assigned;
            }
            catch
            {
                return null;
            }
            finally
            {
                if (job != IntPtr.Zero) CloseHandle(job);
            }
        }

        public bool CloseAndKill()
        {
            IntPtr current = handle;
            if (current == IntPtr.Zero) return false;
            if (!CloseHandle(current)) return false;
            handle = IntPtr.Zero;
            return true;
        }

        public void Dispose()
        {
            CloseAndKill();
            if (handle == IntPtr.Zero) GC.SuppressFinalize(this);
        }

        ~RustGuardWindowsJob()
        {
            CloseAndKill();
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct JOBOBJECT_BASIC_LIMIT_INFORMATION
        {
            public Int64 PerProcessUserTimeLimit;
            public Int64 PerJobUserTimeLimit;
            public UInt32 LimitFlags;
            public UIntPtr MinimumWorkingSetSize;
            public UIntPtr MaximumWorkingSetSize;
            public UInt32 ActiveProcessLimit;
            public UIntPtr Affinity;
            public UInt32 PriorityClass;
            public UInt32 SchedulingClass;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct IO_COUNTERS
        {
            public UInt64 ReadOperationCount;
            public UInt64 WriteOperationCount;
            public UInt64 OtherOperationCount;
            public UInt64 ReadTransferCount;
            public UInt64 WriteTransferCount;
            public UInt64 OtherTransferCount;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION
        {
            public JOBOBJECT_BASIC_LIMIT_INFORMATION BasicLimitInformation;
            public IO_COUNTERS IoInfo;
            public UIntPtr ProcessMemoryLimit;
            public UIntPtr JobMemoryLimit;
            public UIntPtr PeakProcessMemoryUsed;
            public UIntPtr PeakJobMemoryUsed;
        }

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr CreateJobObject(IntPtr securityAttributes, string name);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool SetInformationJobObject(
            IntPtr job,
            UInt32 informationClass,
            ref JOBOBJECT_EXTENDED_LIMIT_INFORMATION information,
            UInt32 informationLength);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool CloseHandle(IntPtr handle);
    }
}
'@
}

function Read-RustGuardStrictUtf8Text {
  param([Parameter(Mandatory = $true)][string]$Path)

  $encoding = New-Object System.Text.UTF8Encoding($false, $true)
  try {
    return $encoding.GetString([System.IO.File]::ReadAllBytes($Path))
  } catch {
    throw "Rust production source is not valid UTF-8: $Path"
  }
}

function Assert-RustGuardProductionSourcePolicy {
  param(
    [Parameter(Mandatory = $true)][System.IO.FileInfo]$File,
    [Parameter(Mandatory = $true)][string]$RepositoryPath,
    [Parameter(Mandatory = $true)][string]$RepoRoot,
    [Parameter(Mandatory = $true)][string]$UnitRoot,
    [Parameter(Mandatory = $true)][string]$SourceRoot
  )

  $content = Read-RustGuardStrictUtf8Text -Path $File.FullName
  $mask = [Stage0.RustGuardLexicalMasker]::Mask($content)
  $hasPathCandidate = [regex]::IsMatch(
    $mask,
    '#\s*\[[^\]]*(?<![A-Za-z0-9_])path\s*=',
    [System.Text.RegularExpressions.RegexOptions]::Singleline
  )
  $hasIncludeCandidate = [regex]::IsMatch(
    $mask,
    '(?<![A-Za-z0-9_])include\s*!\s*\('
  )
  if (-not $hasPathCandidate -and -not $hasIncludeCandidate) {
    return
  }

  $productionChars = $mask.ToCharArray()
  $testPathModulePattern = '(?ms)#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*#\s*\[\s*path\s*=\s*"([^"\r\n]+)"\s*\]\s*(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+((?:r#)?[A-Za-z_][A-Za-z0-9_]*)\s*;'
  foreach ($match in [regex]::Matches($content, $testPathModulePattern)) {
    if ($productionChars[$match.Index] -ne '#') {
      continue
    }

    $declaredPath = $match.Groups[1].Value
    if ([System.IO.Path]::IsPathRooted($declaredPath)) {
      throw "cfg(test) #[path] target must be relative: $RepositoryPath -> $declaredPath"
    }
    try {
      $targetPath = [System.IO.Path]::GetFullPath((Join-Path $File.DirectoryName $declaredPath))
    } catch {
      throw "cfg(test) #[path] target is invalid: $RepositoryPath -> $declaredPath"
    }
    if (-not (Test-Path -LiteralPath $targetPath -PathType Leaf)) {
      throw "cfg(test) #[path] target is missing or not a file: $RepositoryPath -> $declaredPath"
    }
    if (Test-RustGuardPathContainsReparsePoint -RootPath $UnitRoot -TargetPath $targetPath) {
      throw "cfg(test) #[path] target crosses a reparse point or leaves its owning workspace package: $RepositoryPath -> $declaredPath"
    }
    [void](Get-RustGuardRepositoryRelativePath -RepoRoot $RepoRoot -FullName $targetPath)

    $source = [System.IO.Path]::GetFullPath($SourceRoot).TrimEnd([char[]]@('\', '/'))
    $sourcePrefix = $source + [System.IO.Path]::DirectorySeparatorChar
    $insideSource = $targetPath.StartsWith($sourcePrefix, (Get-RustGuardFileSystemStringComparison))
    $testsUnit = [System.IO.Path]::GetFullPath((Join-Path $UnitRoot 'tests/unit')).TrimEnd([char[]]@('\', '/'))
    $testsUnitPrefix = $testsUnit + [System.IO.Path]::DirectorySeparatorChar
    $insideTestsUnit = $targetPath.StartsWith($testsUnitPrefix, (Get-RustGuardFileSystemStringComparison))
    $moduleName = $match.Groups[2].Value
    if ($moduleName.StartsWith('r#', [System.StringComparison]::Ordinal)) {
      $moduleName = $moduleName.Substring(2)
    }

    $historicalSrcTest = $insideSource -and
      [System.IO.Path]::GetExtension($targetPath) -ceq '.rs'
    $governedUnitBridge = $insideTestsUnit -and $moduleName -ceq 'tests'
    if (-not $historicalSrcTest -and -not $governedUnitBridge) {
      throw "cfg(test) #[path] is only allowed for an existing src/*.rs test debt file or the governed tests/unit mod tests bridge: $RepositoryPath -> $declaredPath"
    }

    for ($index = $match.Index; $index -lt $match.Index + $match.Length; $index++) {
      if ($productionChars[$index] -ne "`r" -and $productionChars[$index] -ne "`n") {
        $productionChars[$index] = ' '
      }
    }
  }
  $productionMask = -join $productionChars

  if ([regex]::IsMatch($productionMask, '#\s*\[[^\]]*(?<![A-Za-z0-9_])path\s*=')) {
    throw "production Rust #[path] modules are prohibited; move the module under src or use the governed cfg(test) tests/unit bridge: $RepositoryPath"
  }
  if ([regex]::IsMatch($productionMask, '(?<![A-Za-z0-9_])include\s*!\s*\(')) {
    throw "production Rust include! source injection is prohibited; include_str! and include_bytes! remain allowed: $RepositoryPath"
  }
}

function Get-RustGuardWindowsProcessSnapshot {
  $instances = if ($null -ne (Get-Command Get-CimInstance -ErrorAction SilentlyContinue)) {
    @(Get-CimInstance -ClassName Win32_Process -Property ProcessId,ParentProcessId,CreationDate -ErrorAction Stop)
  } else {
    @(Get-WmiObject -Class Win32_Process -Property ProcessId,ParentProcessId,CreationDate -ErrorAction Stop)
  }
  $byPid = New-Object 'System.Collections.Generic.Dictionary[int,object]'
  foreach ($instance in $instances) {
    $pidValue = 0
    $parentPidValue = 0
    if (-not [int]::TryParse([string]$instance.ProcessId, [ref]$pidValue) -or
        $pidValue -le 0 -or
        -not [int]::TryParse([string]$instance.ParentProcessId, [ref]$parentPidValue) -or
        $parentPidValue -lt 0) {
      continue
    }
    try {
      $startedUtc = if ($instance.CreationDate -is [DateTime]) {
        ([DateTime]$instance.CreationDate).ToUniversalTime()
      } else {
        [System.Management.ManagementDateTimeConverter]::ToDateTime([string]$instance.CreationDate).ToUniversalTime()
      }
    } catch {
      continue
    }
    if ($byPid.ContainsKey($pidValue)) {
      throw "Windows process snapshot contains duplicate PID identity: $pidValue"
    }
    $byPid.Add($pidValue, [PSCustomObject]@{
      Pid = $pidValue
      ParentPid = $parentPidValue
      StartedUtc = $startedUtc
    })
  }
  return [PSCustomObject]@{
    CapturedUtc = [DateTime]::UtcNow
    ByPid = $byPid
  }
}

function Stop-RustGuardWindowsProcessTreeBySnapshot {
  param([Parameter(Mandatory = $true)][System.Diagnostics.Process]$Process)

  $rootPid = [int]$Process.Id
  $rootStartedUtc = $Process.StartTime.ToUniversalTime()
  $initialSnapshot = Get-RustGuardWindowsProcessSnapshot
  $rootRecord = $null
  if ($initialSnapshot.ByPid.ContainsKey($rootPid)) {
    $rootRecord = $initialSnapshot.ByPid[$rootPid]
    if ([Math]::Abs(($rootRecord.StartedUtc - $rootStartedUtc).TotalMilliseconds) -gt 10) {
      throw "Windows process-tree fallback refused reused root PID $rootPid"
    }
  } elseif (-not $Process.HasExited) {
    throw "Windows process-tree fallback could not find root PID $rootPid in its bounded snapshot"
  }

  $known = New-Object 'System.Collections.Generic.Dictionary[int,object]'
  $rootParentPid = if ($null -eq $rootRecord) { 0 } else { $rootRecord.ParentPid }
  $rootTerminatedUtc = if ($Process.HasExited) {
    $Process.ExitTime.ToUniversalTime()
  } else {
    $null
  }
  $known.Add($rootPid, [PSCustomObject]@{
    Pid = $rootPid
    ParentPid = $rootParentPid
    StartedUtc = $rootStartedUtc
    Depth = 0
    TerminatedUtc = $rootTerminatedUtc
  })
  $terminationDeadlineUtc = [DateTime]::UtcNow.AddSeconds(10)
  $extendKnown = {
    param($Snapshot)

    $changed = $true
    while ($changed) {
      $changed = $false
      foreach ($record in @($Snapshot.ByPid.Values)) {
        if ($known.ContainsKey($record.Pid) -or -not $known.ContainsKey($record.ParentPid)) {
          continue
        }
        $parent = $known[$record.ParentPid]
        if ($record.StartedUtc -lt $parent.StartedUtc -or
            $record.StartedUtc -lt $rootStartedUtc -or
            $record.StartedUtc -gt $terminationDeadlineUtc -or
            ($null -ne $parent.TerminatedUtc -and
              $record.StartedUtc -gt $parent.TerminatedUtc.AddMilliseconds(100))) {
          continue
        }
        if ($Snapshot.ByPid.ContainsKey($record.ParentPid)) {
          $liveParent = $Snapshot.ByPid[$record.ParentPid]
          if ([Math]::Abs(($liveParent.StartedUtc - $parent.StartedUtc).TotalMilliseconds) -gt 10) {
            continue
          }
        }
        $known.Add($record.Pid, [PSCustomObject]@{
          Pid = $record.Pid
          ParentPid = $record.ParentPid
          StartedUtc = $record.StartedUtc
          Depth = $parent.Depth + 1
          TerminatedUtc = $null
        })
        $changed = $true
      }
    }
  }
  & $extendKnown $initialSnapshot

  if (-not $Process.HasExited) {
    $actualRootStartedUtc = $Process.StartTime.ToUniversalTime()
    if ([Math]::Abs(($actualRootStartedUtc - $rootStartedUtc).TotalMilliseconds) -gt 10) {
      throw "Windows process-tree fallback refused reused root PID $rootPid before termination"
    }
    $Process.Kill()
    if (-not $Process.WaitForExit(5000)) {
      throw "Windows process-tree fallback could not terminate root PID $rootPid"
    }
    $known[$rootPid].TerminatedUtc = $Process.ExitTime.ToUniversalTime()
  } elseif ($null -eq $known[$rootPid].TerminatedUtc) {
    $known[$rootPid].TerminatedUtc = [DateTime]::UtcNow
  }

  $quietRounds = 0
  foreach ($round in 1..10) {
    $snapshot = Get-RustGuardWindowsProcessSnapshot
    & $extendKnown $snapshot
    $descendants = @(
      foreach ($record in @($known.Values)) {
        if ($record.Pid -eq $rootPid -or -not $snapshot.ByPid.ContainsKey($record.Pid)) {
          continue
        }
        $liveRecord = $snapshot.ByPid[$record.Pid]
        if ([Math]::Abs(($liveRecord.StartedUtc - $record.StartedUtc).TotalMilliseconds) -gt 10) {
          throw "Windows process-tree fallback refused reused descendant PID $($record.Pid)"
        }
        $record
      }
    ) | Sort-Object -Property @{ Expression = 'Depth'; Descending = $true }, @{ Expression = 'Pid'; Descending = $true }
    $descendants = @($descendants)

    if ($descendants.Count -eq 0) {
      $quietRounds++
      if ($quietRounds -ge 3) {
        return 'bounded PID/creation-time process tree fallback'
      }
      Start-Sleep -Milliseconds 100
      continue
    }
    $quietRounds = 0
    foreach ($candidate in $descendants) {
      try {
        $candidateProcess = [System.Diagnostics.Process]::GetProcessById([int]$candidate.Pid)
      } catch [System.ArgumentException] {
        continue
      }
      try {
        $actualStartedUtc = $candidateProcess.StartTime.ToUniversalTime()
        if ([Math]::Abs(($actualStartedUtc - $candidate.StartedUtc).TotalMilliseconds) -gt 10) {
          throw "Windows process-tree fallback refused reused descendant PID $($candidate.Pid)"
        }
        if (-not $candidateProcess.HasExited) {
          $candidateProcess.Kill()
          if (-not $candidateProcess.WaitForExit(5000)) {
            throw "Windows process-tree fallback could not terminate descendant PID $($candidate.Pid)"
          }
        }
        $known[$candidate.Pid].TerminatedUtc = [DateTime]::UtcNow
      } finally {
        $candidateProcess.Dispose()
      }
    }
    Start-Sleep -Milliseconds 100
  }

  throw "Windows process-tree fallback did not reach a stable empty descendant set for root PID $rootPid"
}

function Stop-RustGuardProcessTree {
  param(
    [Parameter(Mandatory = $true)][System.Diagnostics.Process]$Process,
    [AllowNull()][Stage0.RustGuardWindowsJob]$WindowsJob = $null,
    [switch]$DisableTaskkill
  )

  $treeTermination = $null
  if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT -and
      -not $Process.HasExited -and
      $null -ne $WindowsJob -and
      -not $DisableTaskkill) {
    $taskkillPath = $null
    if (-not [string]::IsNullOrWhiteSpace($env:SystemRoot)) {
      $candidate = Join-Path $env:SystemRoot 'System32/taskkill.exe'
      if (Test-Path -LiteralPath $candidate -PathType Leaf) {
        $taskkillPath = $candidate
      }
    }

    if ($null -ne $taskkillPath) {
      $killer = New-Object System.Diagnostics.Process
      try {
        $killer.StartInfo.FileName = $taskkillPath
        $killer.StartInfo.Arguments = '/PID {0} /T /F' -f [int]$Process.Id
        $killer.StartInfo.UseShellExecute = $false
        $killer.StartInfo.CreateNoWindow = $true
        $killer.StartInfo.RedirectStandardOutput = $true
        $killer.StartInfo.RedirectStandardError = $true
        if ($killer.Start()) {
          $killerStdout = $killer.StandardOutput.ReadToEndAsync()
          $killerStderr = $killer.StandardError.ReadToEndAsync()
          if ($killer.WaitForExit(10000)) {
            $killer.WaitForExit()
            $killerTasks = [System.Threading.Tasks.Task[]]@($killerStdout, $killerStderr)
            [void][System.Threading.Tasks.Task]::WaitAll($killerTasks, 5000)
            if ($killer.ExitCode -eq 0) {
              $treeTermination = 'taskkill exact-PID process tree'
            }
          } else {
            try {
              $killer.Kill()
            } catch {
              # The bounded helper may have exited between checks.
            }
          }
        }
      } catch {
        # Fall through to the dedicated Job Object or bounded PID tree.
      } finally {
        $killer.Dispose()
      }
    }
  }

  if ($null -ne $WindowsJob) {
    if ($WindowsJob.CloseAndKill() -and $null -eq $treeTermination) {
      $treeTermination = 'Windows Job Object kill-on-close process tree'
    }
  }
  if ($null -eq $treeTermination -and
      [Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) {
    $treeTermination = Stop-RustGuardWindowsProcessTreeBySnapshot -Process $Process
  }
  if ($null -eq $treeTermination -and
      [Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    if (-not $Process.HasExited) {
      $Process.Kill()
      [void]$Process.WaitForExit(5000)
    }
    $treeTermination = 'non-Windows exact parent process'
  }
  if ($null -eq $treeTermination) {
    $treeTermination = 'process tree already exited'
  }

  return $treeTermination
}

function Invoke-RustGuardProcess {
  param(
    [Parameter(Mandatory = $true)][System.Diagnostics.ProcessStartInfo]$StartInfo,
    [Parameter(Mandatory = $true)][int]$TimeoutMilliseconds,
    [Parameter(Mandatory = $true)][string]$TimeoutContext,
    [switch]$DisableTaskkill,
    [switch]$DisableJobObject
  )

  if ($TimeoutMilliseconds -le 0) {
    throw "Rust guard process timeout must be positive: $TimeoutMilliseconds"
  }

  $process = New-Object System.Diagnostics.Process
  $process.StartInfo = $StartInfo
  $started = $false
  $windowsJob = $null
  try {
    if (-not $process.Start()) {
      throw "$TimeoutContext process did not start"
    }
    $started = $true
    if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT -and
        -not $DisableJobObject) {
      $windowsJob = [Stage0.RustGuardWindowsJob]::TryAssign($process)
    }
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()

    if (-not $process.WaitForExit($TimeoutMilliseconds)) {
      $termination = Stop-RustGuardProcessTree `
        -Process $process `
        -WindowsJob $windowsJob `
        -DisableTaskkill:$DisableTaskkill
      $tasks = [System.Threading.Tasks.Task[]]@($stdoutTask, $stderrTask)
      try {
        [void][System.Threading.Tasks.Task]::WaitAll($tasks, 5000)
      } catch {
        # Preserve the typed process timeout even if a redirected stream faults.
      }
      throw [System.TimeoutException]::new(
        "$TimeoutContext timed out after $TimeoutMilliseconds ms; cargo may be blocked on a package-cache or build-directory lock; termination=$termination"
      )
    }

    # The parameterless wait flushes asynchronous stream notifications on
    # .NET Framework after the process handle has signalled completion.
    $process.WaitForExit()
    $tasks = [System.Threading.Tasks.Task[]]@($stdoutTask, $stderrTask)
    if (-not [System.Threading.Tasks.Task]::WaitAll($tasks, 5000)) {
      throw [System.TimeoutException]::new(
        "$TimeoutContext exited but redirected output did not drain within 5000 ms"
      )
    }

    return [PSCustomObject]@{
      ExitCode = $process.ExitCode
      Stdout = [string]$stdoutTask.Result
      Stderr = [string]$stderrTask.Result
    }
  } finally {
    if ($started) {
      try {
        if (-not $process.HasExited) {
          [void](Stop-RustGuardProcessTree `
            -Process $process `
            -WindowsJob $windowsJob `
            -DisableTaskkill:$DisableTaskkill)
        }
      } catch {
        # Failures here must not hide the primary process/timeout exception.
      }
    }
    if ($null -ne $windowsJob) {
      $windowsJob.Dispose()
    }
    $process.Dispose()
  }
}

function Get-RustGuardMetadataTimeoutMilliseconds {
  $configured = [Environment]::GetEnvironmentVariable('RUST_GUARD_METADATA_TIMEOUT_MS')
  if ([string]::IsNullOrWhiteSpace($configured)) {
    return 30000
  }

  $parsed = 0
  if ($configured -notmatch '^[1-9][0-9]*$' -or
      -not [int]::TryParse($configured, [ref]$parsed) -or
      $parsed -lt 100 -or
      $parsed -gt 300000) {
    throw 'RUST_GUARD_METADATA_TIMEOUT_MS must be a canonical integer from 100 through 300000'
  }
  return $parsed
}

function Invoke-RustGuardCargoMetadata {
  param([Parameter(Mandatory = $true)][string]$RepoRoot)

  $root = [System.IO.Path]::GetFullPath($RepoRoot)
  if ($script:RustGuardMetadataCache.ContainsKey($root)) {
    return $script:RustGuardMetadataCache[$root]
  }

  $cargo = Get-Command cargo -ErrorAction Stop
  $manifest = Join-Path $root 'Cargo.toml'
  if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) {
    throw "Rust workspace manifest is missing: $manifest"
  }

  $startInfo = New-Object System.Diagnostics.ProcessStartInfo
  $startInfo.FileName = $cargo.Source
  $startInfo.Arguments = 'metadata --no-deps --format-version 1 --manifest-path "' + $manifest.Replace('"', '\"') + '"'
  $startInfo.WorkingDirectory = $root
  $startInfo.UseShellExecute = $false
  $startInfo.CreateNoWindow = $true
  $startInfo.RedirectStandardOutput = $true
  $startInfo.RedirectStandardError = $true

  $result = Invoke-RustGuardProcess `
    -StartInfo $startInfo `
    -TimeoutMilliseconds (Get-RustGuardMetadataTimeoutMilliseconds) `
    -TimeoutContext 'cargo metadata'
  if ($result.ExitCode -ne 0) {
    throw "cargo metadata failed with exit code $($result.ExitCode): $($result.Stderr)"
  }

  try {
    $document = $result.Stdout | ConvertFrom-Json
  } catch {
    throw "cargo metadata returned invalid JSON: $($_.Exception.Message)"
  }
  $script:RustGuardMetadataCache[$root] = $document
  return $document
}

function Get-RustGuardWorkspaceUnits {
  param(
    [Parameter(Mandatory = $true)][string]$RepoRoot,
    [AllowNull()]$MetadataDocument = $null
  )

  $root = [System.IO.Path]::GetFullPath($RepoRoot)
  if ($null -eq $MetadataDocument) {
    $MetadataDocument = Invoke-RustGuardCargoMetadata -RepoRoot $root
  }
  if ($null -eq $MetadataDocument -or
      $null -eq $MetadataDocument.workspace_members -or
      $null -eq $MetadataDocument.packages) {
    throw 'cargo metadata document is missing workspace_members or packages'
  }

  $packagesById = New-RustGuardOrdinalDictionary
  foreach ($package in @($MetadataDocument.packages)) {
    $id = [string]$package.id
    if ([string]::IsNullOrWhiteSpace($id) -or $packagesById.ContainsKey($id)) {
      throw "cargo metadata contains an empty or duplicate package id: $id"
    }
    $packagesById.Add($id, $package)
  }

  $memberRecords = @()
  $allRoots = New-RustGuardFileIdentityDictionary
  foreach ($memberIdValue in @($MetadataDocument.workspace_members)) {
    $memberId = [string]$memberIdValue
    if (-not $packagesById.ContainsKey($memberId)) {
      throw "cargo metadata workspace member has no package record: $memberId"
    }
    $package = $packagesById[$memberId]
    if ($package.PSObject.Properties.Name -cnotcontains 'manifest_path' -or
        [string]::IsNullOrWhiteSpace([string]$package.manifest_path) -or
        -not [System.IO.Path]::IsPathRooted([string]$package.manifest_path)) {
      throw "cargo metadata workspace package has an invalid manifest_path: $($package.name)"
    }
    $manifestPath = [System.IO.Path]::GetFullPath([string]$package.manifest_path)
    if ([System.IO.Path]::GetFileName($manifestPath) -cne 'Cargo.toml') {
      throw "workspace package manifest must be named Cargo.toml: $manifestPath"
    }
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
      throw "workspace package manifest is missing or not a file: $manifestPath"
    }
    if (Test-RustGuardPathContainsReparsePoint -RootPath $root -TargetPath $manifestPath) {
      throw "workspace package manifest crosses a reparse point or leaves the repository: $manifestPath"
    }
    $unitRoot = [System.IO.Path]::GetDirectoryName($manifestPath)
    $relativeRoot = Get-RustGuardRepositoryRelativePath -RepoRoot $root -FullName $unitRoot
    $canonicalRoot = [System.IO.Path]::GetFullPath($unitRoot).TrimEnd([char[]]@('\', '/'))
    if ($allRoots.ContainsKey($canonicalRoot)) {
      throw "cargo metadata contains duplicate workspace package roots: $relativeRoot"
    }
    $record = [PSCustomObject]@{
      PackageId = $memberId
      PackageName = [string]$package.name
      Package = $package
      RelativeRoot = $relativeRoot
      UnitRoot = $canonicalRoot
      SourceRoot = (Join-Path $canonicalRoot 'src')
    }
    $allRoots.Add($canonicalRoot, $record)
    $memberRecords += $record
  }

  foreach ($outer in $memberRecords) {
    $outerSource = [System.IO.Path]::GetFullPath($outer.SourceRoot).TrimEnd([char[]]@('\', '/'))
    $outerPrefix = $outerSource + [System.IO.Path]::DirectorySeparatorChar
    foreach ($inner in $memberRecords) {
      if ($inner.PackageId -ceq $outer.PackageId) {
        continue
      }
      $innerRoot = [System.IO.Path]::GetFullPath($inner.UnitRoot).TrimEnd([char[]]@('\', '/'))
      if ($innerRoot.Equals($outerSource, (Get-RustGuardFileSystemStringComparison)) -or
          $innerRoot.StartsWith($outerPrefix, (Get-RustGuardFileSystemStringComparison))) {
        throw "unsupported nested workspace member layout: package '$($inner.PackageName)' root '$($inner.RelativeRoot)' is inside package '$($outer.PackageName)' source directory '$($outer.RelativeRoot)/src'"
      }
    }
  }

  $unitsByRoot = New-RustGuardOrdinalDictionary
  foreach ($record in $memberRecords) {
    if ($record.RelativeRoot -ceq 'crates/evtx-patched') {
      if ($record.PackageName -cne 'evtx') {
        throw "vendored exemption path is owned by an unexpected package: $($record.PackageName)"
      }
      continue
    }
    if ($unitsByRoot.ContainsKey($record.RelativeRoot)) {
      throw "cargo metadata contains duplicate workspace package roots: $($record.RelativeRoot)"
    }
    $unitsByRoot.Add($record.RelativeRoot, $record)
  }

  $units = @()
  foreach ($path in (Get-RustGuardOrdinalSortedStrings -Values ([string[]]@($unitsByRoot.Keys)))) {
    $units += $unitsByRoot[$path]
  }
  return $units
}

function Get-RustGuardSourceRustFilesForUnit {
  param(
    [Parameter(Mandatory = $true)]$Unit
  )

  $sourceRoot = [System.IO.Path]::GetFullPath($Unit.SourceRoot).TrimEnd([char[]]@('\', '/'))
  if (-not (Test-Path -LiteralPath $sourceRoot)) {
    return @()
  }
  if (-not (Test-Path -LiteralPath $sourceRoot -PathType Container)) {
    throw "workspace package src path is not a directory: $($Unit.RelativeRoot)/src"
  }
  if (Test-RustGuardPathContainsReparsePoint -RootPath $Unit.UnitRoot -TargetPath $sourceRoot) {
    throw "workspace package src directory crosses a reparse point: $($Unit.RelativeRoot)/src"
  }

  $files = @()
  $pending = New-Object 'System.Collections.Generic.Stack[System.IO.DirectoryInfo]'
  $pending.Push((Get-Item -LiteralPath $sourceRoot -Force))
  while ($pending.Count -gt 0) {
    $directory = $pending.Pop()
    foreach ($child in @(Get-ChildItem -LiteralPath $directory.FullName -Directory -Force -ErrorAction Stop)) {
      if (($child.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        $relative = Get-RustGuardRepositoryRelativePath -RepoRoot $Unit.UnitRoot -FullName $child.FullName
        throw "workspace package src tree crosses a reparse point: $($Unit.RelativeRoot)/$relative"
      }
      $pending.Push($child)
    }

    foreach ($file in @(Get-ChildItem -LiteralPath $directory.FullName -File -Force -ErrorAction Stop)) {
      if ([System.IO.Path]::GetExtension($file.Name) -ine '.rs') {
        continue
      }
      if (($file.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "workspace package src file is a reparse point: $($file.FullName)"
      }
      $files += $file
    }
  }

  return $files
}

function Resolve-RustGuardProductionTargetFile {
  param(
    [Parameter(Mandatory = $true)][string]$RepoRoot,
    [Parameter(Mandatory = $true)]$Unit,
    [Parameter(Mandatory = $true)]$Target
  )

  if ($Target.PSObject.Properties.Name -cnotcontains 'src_path' -or
      [string]::IsNullOrWhiteSpace([string]$Target.src_path)) {
    throw "cargo metadata production target is missing src_path for package $($Unit.PackageName)"
  }
  if (-not [System.IO.Path]::IsPathRooted([string]$Target.src_path)) {
    throw "cargo metadata production target src_path must be absolute for package $($Unit.PackageName): $($Target.src_path)"
  }
  try {
    $targetPath = [System.IO.Path]::GetFullPath([string]$Target.src_path)
  } catch {
    throw "cargo metadata production target has an invalid src_path for package $($Unit.PackageName): $($Target.src_path)"
  }
  if (-not (Test-Path -LiteralPath $targetPath -PathType Leaf)) {
    throw "cargo metadata production target src_path is not an existing file for package $($Unit.PackageName): $targetPath"
  }

  $unitRoot = [System.IO.Path]::GetFullPath($Unit.UnitRoot).TrimEnd([char[]]@('\', '/'))
  $sourceRoot = [System.IO.Path]::GetFullPath($Unit.SourceRoot).TrimEnd([char[]]@('\', '/'))
  $sourcePrefix = $sourceRoot + [System.IO.Path]::DirectorySeparatorChar
  $buildScript = [System.IO.Path]::GetFullPath((Join-Path $unitRoot 'build.rs'))
  $insideSource = $targetPath.StartsWith($sourcePrefix, (Get-RustGuardFileSystemStringComparison))
  $isRootBuildScript = $targetPath.Equals($buildScript, (Get-RustGuardFileSystemStringComparison))
  if (-not $insideSource -and -not $isRootBuildScript) {
    throw "cargo metadata production target src_path must be inside its owning package src directory; only the exact package-root build.rs is allowed outside src: package=$($Unit.PackageName), target=$targetPath"
  }
  if (Test-RustGuardPathContainsReparsePoint -RootPath $unitRoot -TargetPath $targetPath) {
    throw "cargo metadata production target src_path crosses a reparse point or leaves its owning package for package $($Unit.PackageName): $targetPath"
  }
  [void](Get-RustGuardRepositoryRelativePath -RepoRoot $RepoRoot -FullName $targetPath)

  return [System.IO.FileInfo](Get-Item -LiteralPath $targetPath -Force)
}

function Get-RustGuardFiles {
  param(
    [Parameter(Mandatory = $true)][string]$RepoRoot,
    [Parameter(Mandatory = $true)][ValidateSet('Production', 'TestLayout')][string]$Mode,
    [AllowNull()]$MetadataDocument = $null
  )

  $filesByIdentity = New-RustGuardFileIdentityDictionary
  $productionIdentities = New-RustGuardFileIdentityDictionary
  $workspaceUnits = @(Get-RustGuardWorkspaceUnits -RepoRoot $RepoRoot -MetadataDocument $MetadataDocument)
  foreach ($unit in $workspaceUnits) {
    if ($unit.Package.PSObject.Properties.Name -cnotcontains 'targets') {
      throw "cargo metadata package is missing targets: $($unit.PackageName)"
    }
    foreach ($file in @(Get-RustGuardSourceRustFilesForUnit -Unit $unit)) {
      $relative = Get-RustGuardRepositoryRelativePath -RepoRoot $RepoRoot -FullName $file.FullName
      $identity = [System.IO.Path]::GetFullPath($file.FullName)
      if ($filesByIdentity.ContainsKey($identity)) {
        throw "Rust workspace scanner produced a duplicate physical source identity: $relative"
      }
      $filesByIdentity.Add($identity, [PSCustomObject]@{
        File = $file
        Path = $relative
        UnitRoot = $unit.UnitRoot
        SourceRoot = $unit.SourceRoot
      })
      $productionIdentities[$identity] = $true
    }

    foreach ($target in @($unit.Package.targets)) {
      if ($target.PSObject.Properties.Name -cnotcontains 'kind') {
        throw "cargo metadata target is missing kind for package $($unit.PackageName)"
      }
      $targetKinds = [string[]]@($target.kind)
      if ($targetKinds -ccontains 'test' -or
          $targetKinds -ccontains 'bench' -or
          $targetKinds -ccontains 'example') {
        continue
      }
      $file = Resolve-RustGuardProductionTargetFile -RepoRoot $RepoRoot -Unit $unit -Target $target
      $relative = Get-RustGuardRepositoryRelativePath -RepoRoot $RepoRoot -FullName $file.FullName
      $identity = [System.IO.Path]::GetFullPath($file.FullName)
      if (-not $filesByIdentity.ContainsKey($identity)) {
        $filesByIdentity.Add($identity, [PSCustomObject]@{
          File = $file
          Path = $relative
          UnitRoot = $unit.UnitRoot
          SourceRoot = $unit.SourceRoot
        })
      }
      $productionIdentities[$identity] = $true
    }

    $buildScript = Join-Path $unit.UnitRoot 'build.rs'
    if (Test-Path -LiteralPath $buildScript -PathType Leaf) {
      if (Test-RustGuardPathContainsReparsePoint -RootPath $unit.UnitRoot -TargetPath $buildScript) {
        throw "workspace package build.rs crosses a reparse point: $($unit.RelativeRoot)/build.rs"
      }
      $file = Get-Item -LiteralPath $buildScript -Force
      $relative = Get-RustGuardRepositoryRelativePath -RepoRoot $RepoRoot -FullName $file.FullName
      $identity = [System.IO.Path]::GetFullPath($file.FullName)
      if ($filesByIdentity.ContainsKey($identity)) {
        $productionIdentities[$identity] = $true
        continue
      }
      $filesByIdentity.Add($identity, [PSCustomObject]@{
        File = $file
        Path = $relative
        UnitRoot = $unit.UnitRoot
        SourceRoot = $unit.SourceRoot
      })
      $productionIdentities[$identity] = $true
    }
  }

  foreach ($identity in @($productionIdentities.Keys)) {
    if (-not $filesByIdentity.ContainsKey($identity)) {
      throw "Rust production physical identity was not retained in the unified file set: $identity"
    }
    $entry = $filesByIdentity[$identity]
    Assert-RustGuardProductionSourcePolicy `
      -File $entry.File `
      -RepositoryPath $entry.Path `
      -RepoRoot $RepoRoot `
      -UnitRoot $entry.UnitRoot `
      -SourceRoot $entry.SourceRoot
  }

  $entriesByPath = New-RustGuardOrdinalDictionary
  foreach ($entry in @($filesByIdentity.Values)) {
    if ($entriesByPath.ContainsKey($entry.Path)) {
      throw "Rust workspace scanner produced duplicate stable repository output path: $($entry.Path)"
    }
    $entriesByPath.Add($entry.Path, $entry)
  }
  $files = @()
  foreach ($path in (Get-RustGuardOrdinalSortedStrings -Values ([string[]]@($entriesByPath.Keys)))) {
    $files += $entriesByPath[$path]
  }
  return $files
}

function Test-RustGuardPathContainsReparsePoint {
  param(
    [Parameter(Mandatory = $true)][string]$RootPath,
    [Parameter(Mandatory = $true)][string]$TargetPath
  )

  try {
    $root = [System.IO.Path]::GetFullPath($RootPath).TrimEnd([char[]]@('\', '/'))
    $target = [System.IO.Path]::GetFullPath($TargetPath)
    $prefix = $root + [System.IO.Path]::DirectorySeparatorChar
    if (-not $target.Equals($root, (Get-RustGuardFileSystemStringComparison)) -and
        -not $target.StartsWith($prefix, (Get-RustGuardFileSystemStringComparison))) {
      return $true
    }

    $rootItem = Get-Item -LiteralPath $root -Force -ErrorAction Stop
    if (($rootItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
      return $true
    }

    if ($target.Equals($root, (Get-RustGuardFileSystemStringComparison))) {
      return $false
    }
    $relative = $target.Substring($prefix.Length)
    $current = $root
    foreach ($component in @($relative -split '[\\/]')) {
      if ([string]::IsNullOrWhiteSpace($component)) {
        return $true
      }
      $current = Join-Path $current $component
      $item = Get-Item -LiteralPath $current -Force -ErrorAction Stop
      if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        return $true
      }
    }
    return $false
  } catch {
    return $true
  }
}

function ConvertTo-RustGuardCanonicalInt {
  param(
    [Parameter(Mandatory = $true)]$Value,
    [Parameter(Mandatory = $true)][string]$Field,
    [Parameter(Mandatory = $true)][string]$Identity,
    [switch]$AllowZero
  )

  $text = [string]$Value
  $pattern = if ($AllowZero) { '^(0|[1-9][0-9]*)$' } else { '^[1-9][0-9]*$' }
  $parsed = 0
  if ($text -notmatch $pattern -or -not [int]::TryParse($text, [ref]$parsed)) {
    throw "Invalid canonical integer for $Identity`: $Field=$Value"
  }
  return $parsed
}

function Format-RustGuardCsvField {
  param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Value)

  if ($Value -match '[,"\r\n]') {
    return '"' + $Value.Replace('"', '""') + '"'
  }
  return $Value
}

function ConvertFrom-RustGuardCsv {
  param(
    [Parameter(Mandatory = $true)][string]$Content,
    [Parameter(Mandatory = $true)][string]$Header,
    [Parameter(Mandatory = $true)][string]$Source
  )

  if ($Content.Length -eq 0) {
    throw "CSV at $Source must not be empty; use a header-only file for zero debt"
  }
  $firstLine = @($Content -split '\r?\n')[0]
  if ($firstLine -cne $Header) {
    throw "CSV at $Source must use the exact header: $Header"
  }

  $expectedFields = [string[]]@($Header -split ',')
  $rows = @($Content | ConvertFrom-Csv)
  foreach ($row in $rows) {
    $actualFields = [string[]]@($row.PSObject.Properties.Name)
    if ($actualFields.Count -ne $expectedFields.Count) {
      throw "CSV at $Source contains data outside its exact schema"
    }
    for ($index = 0; $index -lt $expectedFields.Count; $index++) {
      if ($actualFields[$index] -cne $expectedFields[$index]) {
        throw "CSV at $Source contains data outside its exact schema"
      }
    }
  }
  return [object[]]$rows
}

function Assert-RustGuardCanonicalCsvText {
  param(
    [Parameter(Mandatory = $true)][string]$Content,
    [Parameter(Mandatory = $true)][string]$Canonical,
    [Parameter(Mandatory = $true)][string]$Source
  )

  $normalized = $Content.Replace("`r`n", "`n")
  if ($normalized.Contains("`r")) {
    throw "CSV at $Source contains a non-canonical carriage return"
  }
  if ($normalized.EndsWith("`n")) {
    $normalized = $normalized.Substring(0, $normalized.Length - 1)
  }
  if ($normalized -cne $Canonical) {
    throw "CSV at $Source is not canonical or contains data outside its exact schema"
  }
}

function Assert-RustGuardTrustedBootstrapSha256 {
  param(
    [Parameter(Mandatory = $true)][string]$GuardName,
    [AllowEmptyString()][string]$ExpectedSha256,
    [Parameter(Mandatory = $true)][string]$ManifestSha256,
    [Parameter(Mandatory = $true)][string]$ActualSha256
  )

  if ([string]::IsNullOrWhiteSpace($ExpectedSha256)) {
    throw "$GuardName bootstrap requires a trusted SHA-256 supplied outside the pull request"
  }
  if ($ExpectedSha256 -cnotmatch '^[0-9a-f]{64}$') {
    throw "$GuardName trusted bootstrap SHA-256 is invalid"
  }
  if ($ExpectedSha256 -cne $ManifestSha256 -or $ExpectedSha256 -cne $ActualSha256) {
    throw "$GuardName bootstrap is not authorized by the trusted SHA-256: expected $ExpectedSha256, manifest $ManifestSha256, actual $ActualSha256"
  }
}

function Invoke-RustGuardProcessSelfTest {
  $payload = '$text = "x" * 131072; [Console]::Out.Write($text); [Console]::Error.Write($text)'
  $encodedPayload = [Convert]::ToBase64String([System.Text.Encoding]::Unicode.GetBytes($payload))
  $ioStartInfo = New-Object System.Diagnostics.ProcessStartInfo
  $ioStartInfo.FileName = Join-Path $PSHOME 'powershell.exe'
  $ioStartInfo.Arguments = "-NoProfile -NonInteractive -EncodedCommand $encodedPayload"
  $ioStartInfo.UseShellExecute = $false
  $ioStartInfo.CreateNoWindow = $true
  $ioStartInfo.RedirectStandardOutput = $true
  $ioStartInfo.RedirectStandardError = $true
  $ioResult = Invoke-RustGuardProcess `
    -StartInfo $ioStartInfo `
    -TimeoutMilliseconds 5000 `
    -TimeoutContext 'redirected output self-test'
  if ($ioResult.ExitCode -ne 0 -or
      $ioResult.Stdout.Length -ne 131072 -or
      $ioResult.Stderr.Length -ne 131072) {
    throw "Rust guard concurrent output self-test failed: exit=$($ioResult.ExitCode) stdout=$($ioResult.Stdout.Length) stderr=$($ioResult.Stderr.Length)"
  }

  $unrelatedPayload = '[System.Threading.Thread]::Sleep(60000)'
  $unrelatedEncoded = [Convert]::ToBase64String([System.Text.Encoding]::Unicode.GetBytes($unrelatedPayload))
  $unrelatedProcess = Start-Process `
    -FilePath (Join-Path $PSHOME 'powershell.exe') `
    -ArgumentList "-NoProfile -NonInteractive -EncodedCommand $unrelatedEncoded" `
    -WindowStyle Hidden `
    -PassThru
  try {
    $unrelatedStartedUtc = $unrelatedProcess.StartTime.ToUniversalTime()
    $normalPayload = '[Console]::Out.Write("normal")'
    $normalEncoded = [Convert]::ToBase64String([System.Text.Encoding]::Unicode.GetBytes($normalPayload))
    $normalStartInfo = New-Object System.Diagnostics.ProcessStartInfo
    $normalStartInfo.FileName = Join-Path $PSHOME 'powershell.exe'
    $normalStartInfo.Arguments = "-NoProfile -NonInteractive -EncodedCommand $normalEncoded"
    $normalStartInfo.UseShellExecute = $false
    $normalStartInfo.CreateNoWindow = $true
    $normalStartInfo.RedirectStandardOutput = $true
    $normalStartInfo.RedirectStandardError = $true
    $normalResult = Invoke-RustGuardProcess `
      -StartInfo $normalStartInfo `
      -TimeoutMilliseconds 5000 `
      -TimeoutContext 'normal completion isolation self-test'
    if ($normalResult.ExitCode -ne 0 -or $normalResult.Stdout -cne 'normal') {
      throw 'Normal process completion self-test did not return its expected output'
    }
    if ($unrelatedProcess.HasExited -or
        [Math]::Abs(($unrelatedProcess.StartTime.ToUniversalTime() - $unrelatedStartedUtc).TotalSeconds) -gt 1) {
      throw 'Normal process completion terminated or replaced an unrelated process'
    }
  } finally {
    if (-not $unrelatedProcess.HasExited) {
      $unrelatedProcess.Kill()
      [void]$unrelatedProcess.WaitForExit(5000)
    }
    $unrelatedProcess.Dispose()
  }

  $tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('meow-rust-process-' + [guid]::NewGuid().ToString('N'))
  $ownedPids = New-Object System.Collections.ArrayList
  try {
    [void][System.IO.Directory]::CreateDirectory($tempRoot)
    $fakeCargoPath = Join-Path $tempRoot 'fake-cargo.ps1'
$fakeCargo = @'
param(
  [Parameter(Mandatory = $true)][string]$ParentPidPath,
  [Parameter(Mandatory = $true)][string]$ChildPidPath,
  [string]$SpawnedPidPath,
  [switch]$ContinuouslySpawn,
  [int]$ExitAfterMilliseconds = 0
)
[System.IO.File]::WriteAllText($ParentPidPath, [string]$PID, [System.Text.Encoding]::ASCII)
$lifetime = [System.Diagnostics.Stopwatch]::StartNew()
$childPayload = '[System.Threading.Thread]::Sleep(60000)'
$encodedChild = [Convert]::ToBase64String([System.Text.Encoding]::Unicode.GetBytes($childPayload))
$child = Start-Process `
  -FilePath (Join-Path $PSHOME 'powershell.exe') `
  -ArgumentList "-NoProfile -NonInteractive -EncodedCommand $encodedChild" `
  -WindowStyle Hidden `
  -PassThru
[System.IO.File]::WriteAllText($ChildPidPath, [string]$child.Id, [System.Text.Encoding]::ASCII)
while ($true) {
  if ($ContinuouslySpawn) {
    $extra = Start-Process `
      -FilePath (Join-Path $PSHOME 'powershell.exe') `
      -ArgumentList "-NoProfile -NonInteractive -EncodedCommand $encodedChild" `
      -WindowStyle Hidden `
      -PassThru
    [System.IO.File]::AppendAllText($SpawnedPidPath, [string]$extra.Id + [Environment]::NewLine, [System.Text.Encoding]::ASCII)
  }
  if ($ExitAfterMilliseconds -gt 0 -and $lifetime.ElapsedMilliseconds -ge $ExitAfterMilliseconds) {
    break
  }
  Start-Sleep -Milliseconds 75
}
'@
    [System.IO.File]::WriteAllText($fakeCargoPath, $fakeCargo, (New-Object System.Text.UTF8Encoding($false, $true)))

    $assertProcessGone = {
      param([int]$ProcessId, [string]$Label)

      foreach ($attempt in 1..50) {
        try {
          $candidate = [System.Diagnostics.Process]::GetProcessById($ProcessId)
          try {
            if ($candidate.HasExited) {
              return
            }
          } finally {
            $candidate.Dispose()
          }
        } catch [System.ArgumentException] {
          return
        }
        Start-Sleep -Milliseconds 100
      }
      throw "$Label process remained alive after bounded tree termination: pid=$ProcessId"
    }

    $runTimeoutCase = {
      param(
        [string]$Name,
        [bool]$DisableTaskkill,
        [bool]$DisableJobObject,
        [string]$ExpectedTermination
      )

      $parentPidPath = Join-Path $tempRoot "$Name-parent.pid"
      $childPidPath = Join-Path $tempRoot "$Name-child.pid"
      $spawnedPidPath = Join-Path $tempRoot "$Name-spawned.pid"
      $continuousSpawn = $Name -eq 'snapshot-fallback'
      $startInfo = New-Object System.Diagnostics.ProcessStartInfo
      $startInfo.FileName = Join-Path $PSHOME 'powershell.exe'
      $startInfo.Arguments = '-NoProfile -NonInteractive -File "' + $fakeCargoPath.Replace('"', '\"') + '" -ParentPidPath "' + $parentPidPath.Replace('"', '\"') + '" -ChildPidPath "' + $childPidPath.Replace('"', '\"') + '"'
      if ($continuousSpawn) {
        $startInfo.Arguments += ' -ContinuouslySpawn -SpawnedPidPath "' + $spawnedPidPath.Replace('"', '\"') + '"'
      }
      $startInfo.UseShellExecute = $false
      $startInfo.CreateNoWindow = $true
      $startInfo.RedirectStandardOutput = $true
      $startInfo.RedirectStandardError = $true

      $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
      $timeout = $null
      try {
        [void](Invoke-RustGuardProcess `
          -StartInfo $startInfo `
          -TimeoutMilliseconds 1500 `
          -TimeoutContext "fake cargo $Name self-test" `
          -DisableTaskkill:$DisableTaskkill `
          -DisableJobObject:$DisableJobObject)
      } catch {
        $timeout = $_.Exception
      } finally {
        $stopwatch.Stop()
      }
      if ($null -eq $timeout -or
          -not ($timeout -is [System.TimeoutException]) -or
          $timeout.Message -notmatch 'package-cache or build-directory lock' -or
          $timeout.Message -notmatch $ExpectedTermination -or
          $stopwatch.ElapsedMilliseconds -gt 15000) {
        $message = if ($null -eq $timeout) { '<no timeout>' } else { $timeout.Message }
        throw "Rust guard process-tree timeout self-test failed for $Name`: elapsed=$($stopwatch.ElapsedMilliseconds) message=$message"
      }

      foreach ($pidPath in @($parentPidPath, $childPidPath)) {
        if (-not (Test-Path -LiteralPath $pidPath -PathType Leaf)) {
          throw "Fake cargo did not record its process tree before timeout: $pidPath"
        }
        $pidText = [System.IO.File]::ReadAllText($pidPath, [System.Text.Encoding]::ASCII)
        $parsedPid = 0
        if ($pidText -notmatch '^[1-9][0-9]*$' -or
            -not [int]::TryParse($pidText, [ref]$parsedPid)) {
          throw "Fake cargo recorded an invalid PID: $pidPath=$pidText"
        }
        [void]$ownedPids.Add($parsedPid)
        & $assertProcessGone $parsedPid "$Name/$([System.IO.Path]::GetFileNameWithoutExtension($pidPath))"
      }
      if ($continuousSpawn) {
        if (-not (Test-Path -LiteralPath $spawnedPidPath -PathType Leaf)) {
          throw "Continuous snapshot fallback fixture did not record spawned descendants: $spawnedPidPath"
        }
        foreach ($pidText in @([System.IO.File]::ReadAllLines($spawnedPidPath, [System.Text.Encoding]::ASCII))) {
          $parsedPid = 0
          if ($pidText -notmatch '^[1-9][0-9]*$' -or
              -not [int]::TryParse($pidText, [ref]$parsedPid)) {
            throw "Continuous snapshot fallback fixture recorded an invalid PID: $pidText"
          }
          [void]$ownedPids.Add($parsedPid)
          & $assertProcessGone $parsedPid "$Name/continuous-child"
        }
      }
    }

    if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) {
      & $runTimeoutCase 'taskkill' $false $false 'termination=(taskkill exact-PID process tree|Windows Job Object kill-on-close process tree)'
      & $runTimeoutCase 'job-fallback' $true $false 'termination=Windows Job Object kill-on-close process tree'
      & $runTimeoutCase 'snapshot-fallback' $true $true 'termination=bounded PID/creation-time process tree fallback'

      $exitedRootParentPath = Join-Path $tempRoot 'snapshot-exited-root-parent.pid'
      $exitedRootChildPath = Join-Path $tempRoot 'snapshot-exited-root-child.pid'
      $exitedRootStartInfo = New-Object System.Diagnostics.ProcessStartInfo
      $exitedRootStartInfo.FileName = Join-Path $PSHOME 'powershell.exe'
      $exitedRootStartInfo.Arguments = '-NoProfile -NonInteractive -File "' + $fakeCargoPath.Replace('"', '\"') + '" -ParentPidPath "' + $exitedRootParentPath.Replace('"', '\"') + '" -ChildPidPath "' + $exitedRootChildPath.Replace('"', '\"') + '" -ExitAfterMilliseconds 300'
      $exitedRootStartInfo.UseShellExecute = $false
      $exitedRootStartInfo.CreateNoWindow = $true
      $exitedRootProcess = New-Object System.Diagnostics.Process
      $exitedRootProcess.StartInfo = $exitedRootStartInfo
      try {
        if (-not $exitedRootProcess.Start() -or -not $exitedRootProcess.WaitForExit(5000)) {
          throw 'Exited-root snapshot fallback fixture did not terminate its root process'
        }
        $termination = Stop-RustGuardProcessTree `
          -Process $exitedRootProcess `
          -DisableTaskkill
        if ($termination -ne 'bounded PID/creation-time process tree fallback') {
          throw "Exited-root snapshot fallback returned an unexpected termination mode: $termination"
        }
        foreach ($pidPath in @($exitedRootParentPath, $exitedRootChildPath)) {
          if (-not (Test-Path -LiteralPath $pidPath -PathType Leaf)) {
            throw "Exited-root snapshot fallback fixture did not record PID: $pidPath"
          }
          $pidText = [System.IO.File]::ReadAllText($pidPath, [System.Text.Encoding]::ASCII)
          $parsedPid = 0
          if ($pidText -notmatch '^[1-9][0-9]*$' -or
              -not [int]::TryParse($pidText, [ref]$parsedPid)) {
            throw "Exited-root snapshot fallback fixture recorded an invalid PID: $pidPath=$pidText"
          }
          [void]$ownedPids.Add($parsedPid)
          & $assertProcessGone $parsedPid "snapshot-exited-root/$([System.IO.Path]::GetFileNameWithoutExtension($pidPath))"
        }
      } finally {
        $exitedRootProcess.Dispose()
      }
    }
  } finally {
    foreach ($ownedPid in @($ownedPids)) {
      try {
        $remainingProcess = [System.Diagnostics.Process]::GetProcessById([int]$ownedPid)
        try {
          if (-not $remainingProcess.HasExited) {
            $remainingProcess.Kill()
            [void]$remainingProcess.WaitForExit(5000)
          }
        } finally {
          $remainingProcess.Dispose()
        }
      } catch [System.ArgumentException] {
        # The desired cleanup state is that each owned PID no longer exists.
      }
    }
    if (Test-Path -LiteralPath $tempRoot) {
      Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
  }
}

function Invoke-RustGuardWorkspaceDiscoverySelfTest {
  param(
    [Parameter(Mandatory = $true)][System.Text.Encoding]$Encoding,
    [AllowNull()][scriptblock]$CodeTargetAssertion = $null
  )

  Invoke-RustGuardProcessSelfTest

  $tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('meow-rust-workspace-' + [guid]::NewGuid().ToString('N'))
  $outsideTarget = $null
  try {
    $normalRoot = Join-Path $tempRoot 'crates/normal'
    $vendorRoot = Join-Path $tempRoot 'crates/evtx-patched'
    $rogueRoot = Join-Path $tempRoot 'tools/rogue'
    foreach ($root in @($normalRoot, $vendorRoot, $rogueRoot)) {
      [void][System.IO.Directory]::CreateDirectory((Join-Path $root 'src'))
      [System.IO.File]::WriteAllText((Join-Path $root 'Cargo.toml'), "[package]`nname = `"sample`"`nversion = `"0.0.0`"`n", $Encoding)
      [System.IO.File]::WriteAllText((Join-Path $root 'src/lib.rs'), "fn production() {}`n", $Encoding)
    }
    [System.IO.File]::WriteAllText((Join-Path $rogueRoot 'src/test_helpers.rs'), "#[test]`nfn helper() {}`n", $Encoding)
    [void][System.IO.Directory]::CreateDirectory((Join-Path $rogueRoot 'src/tests'))
    [System.IO.File]::WriteAllText((Join-Path $rogueRoot 'src/tests/hidden.rs'), "#[test]`nfn hidden() {}`n", $Encoding)
    foreach ($physicalTestRoot in @('tests', 'benches', 'examples')) {
      $physicalDirectory = Join-Path $rogueRoot $physicalTestRoot
      [void][System.IO.Directory]::CreateDirectory($physicalDirectory)
      [System.IO.File]::WriteAllText((Join-Path $physicalDirectory 'ignored.rs'), "#[test]`nfn ignored() {}`n", $Encoding)
    }
    [System.IO.File]::WriteAllText((Join-Path $rogueRoot 'build.rs'), "fn main() {}`n", $Encoding)
    $engineRoot = Join-Path $rogueRoot 'src/engine'
    [void][System.IO.Directory]::CreateDirectory($engineRoot)
    $nonstandardTarget = Join-Path $engineRoot 'code.txt'
    $targetLines = New-Object System.Collections.ArrayList
    [void]$targetLines.Add('mod helper;')
    [void]$targetLines.Add('#[test]')
    [void]$targetLines.Add('fn target_test() {}')
    [void]$targetLines.Add('fn target_long() {')
    foreach ($line in 1..105) {
      [void]$targetLines.Add("    let value_$line = $line;")
    }
    [void]$targetLines.Add('}')
    while ($targetLines.Count -lt 510) {
      [void]$targetLines.Add('// target padding')
    }
    $targetLineArray = [string[]]@($targetLines | ForEach-Object { [string]$_ })
    $targetContent = ($targetLineArray -join "`n") + "`n"
    [System.IO.File]::WriteAllText($nonstandardTarget, $targetContent, $Encoding)

    $recursiveModule = Join-Path $engineRoot 'helper.rs'
    $helperLines = New-Object System.Collections.ArrayList
    [void]$helperLines.Add('#[test]')
    [void]$helperLines.Add('fn helper_test() {}')
    [void]$helperLines.Add('fn helper_long() {')
    foreach ($line in 1..105) {
      [void]$helperLines.Add("    let helper_value_$line = $line;")
    }
    [void]$helperLines.Add('}')
    while ($helperLines.Count -lt 510) {
      [void]$helperLines.Add('// helper padding')
    }
    $helperLineArray = [string[]]@($helperLines | ForEach-Object { [string]$_ })
    $helperContent = ($helperLineArray -join "`n") + "`n"
    [System.IO.File]::WriteAllText($recursiveModule, $helperContent, $Encoding)

    $rogueManifest = @'
[package]
name = "rogue-target-self-test"
version = "0.0.0"
edition = "2021"

[lib]
path = "src/engine/code.txt"
'@
    $rogueManifestPath = Join-Path $rogueRoot 'Cargo.toml'
    [System.IO.File]::WriteAllText($rogueManifestPath, $rogueManifest, $Encoding)
    $workspaceManifest = @'
[workspace]
members = ["tools/rogue"]
resolver = "2"
'@
    [System.IO.File]::WriteAllText((Join-Path $tempRoot 'Cargo.toml'), $workspaceManifest, $Encoding)
    $cargo = Get-Command cargo -ErrorAction Stop
    $cargoCheckStart = New-Object System.Diagnostics.ProcessStartInfo
    $cargoCheckStart.FileName = $cargo.Source
    $cargoCheckStart.Arguments = 'check --quiet --manifest-path "' + $rogueManifestPath.Replace('"', '\"') + '" --target-dir "' + (Join-Path $rogueRoot 'target').Replace('"', '\"') + '"'
    $cargoCheckStart.WorkingDirectory = $rogueRoot
    $cargoCheckStart.UseShellExecute = $false
    $cargoCheckStart.CreateNoWindow = $true
    $cargoCheckStart.RedirectStandardOutput = $true
    $cargoCheckStart.RedirectStandardError = $true
    $cargoCheckResult = Invoke-RustGuardProcess `
      -StartInfo $cargoCheckStart `
      -TimeoutMilliseconds 30000 `
      -TimeoutContext 'recursive non-.rs Cargo target self-test'
    if ($cargoCheckResult.ExitCode -ne 0) {
      throw "real Cargo recursive target self-test failed: $($cargoCheckResult.Stderr)"
    }
    $realMetadataFiles = @(Get-RustGuardFiles -RepoRoot $tempRoot -Mode Production)
    $realMetadataPaths = @($realMetadataFiles | ForEach-Object { $_.Path })
    if ($realMetadataPaths -cnotcontains 'tools/rogue/src/engine/code.txt' -or
        $realMetadataPaths -cnotcontains 'tools/rogue/src/engine/helper.rs') {
      throw "real cargo metadata discovery missed the recursive non-.rs target module: $($realMetadataPaths -join ',')"
    }

    $normalId = 'normal-id'
    $vendorId = 'vendor-id'
    $rogueId = 'rogue-id'
    $metadata = [PSCustomObject]@{
      workspace_members = @($normalId, $vendorId, $rogueId)
      packages = @(
        [PSCustomObject]@{ id = $normalId; name = 'normal'; manifest_path = (Join-Path $normalRoot 'Cargo.toml'); targets = @() },
        [PSCustomObject]@{ id = $vendorId; name = 'evtx'; manifest_path = (Join-Path $vendorRoot 'Cargo.toml'); targets = @() },
        [PSCustomObject]@{
          id = $rogueId
          name = 'rogue'
          manifest_path = (Join-Path $rogueRoot 'Cargo.toml')
          targets = @(
            [PSCustomObject]@{ kind = @('lib'); src_path = $nonstandardTarget },
            [PSCustomObject]@{ kind = @('custom-build'); src_path = (Join-Path $rogueRoot 'build.rs') }
          )
        }
      )
    }

    $units = @(Get-RustGuardWorkspaceUnits -RepoRoot $tempRoot -MetadataDocument $metadata)
    $roots = @($units | ForEach-Object { $_.RelativeRoot })
    if (($roots -join ',') -cne 'crates/normal,tools/rogue') {
      throw "workspace discovery did not include tools/rogue or did not exclude the exact vendor: $($roots -join ',')"
    }

    $production = @(Get-RustGuardFiles -RepoRoot $tempRoot -Mode Production -MetadataDocument $metadata)
    $paths = @($production | ForEach-Object { $_.Path })
    if ($paths -cnotcontains 'tools/rogue/src/lib.rs' -or
        $paths -cnotcontains 'tools/rogue/build.rs' -or
        $paths -cnotcontains 'tools/rogue/src/engine/code.txt' -or
        $paths -cnotcontains 'tools/rogue/src/engine/helper.rs' -or
        $paths -cnotcontains 'tools/rogue/src/test_helpers.rs' -or
        $paths -cnotcontains 'tools/rogue/src/tests/hidden.rs' -or
        $paths -ccontains 'tools/rogue/tests/ignored.rs' -or
        $paths -ccontains 'tools/rogue/benches/ignored.rs' -or
        $paths -ccontains 'tools/rogue/examples/ignored.rs') {
      throw "workspace production boundary self-test failed: $($paths -join ',')"
    }

    $identityProbe = New-RustGuardFileIdentityDictionary
    $identityProbe['C:/probe/src/lib.rs'] = $true
    $identityProbe['C:/probe/SRC/LIB.RS'] = $true
    $expectedIdentityCount = if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) { 1 } else { 2 }
    if ($identityProbe.Count -ne $expectedIdentityCount) {
      throw "filesystem path identity comparer is not platform-correct: expected=$expectedIdentityCount actual=$($identityProbe.Count)"
    }

    if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) {
      $caseOriginalTargets = @($metadata.packages[2].targets)
      try {
        $caseAliasTarget = Join-Path $rogueRoot 'SRC/LIB.RS'
        $metadata.packages[2].targets = @($caseOriginalTargets) + @(
          [PSCustomObject]@{ kind = @('bin'); src_path = $caseAliasTarget }
        )
        $caseProduction = @(Get-RustGuardFiles -RepoRoot $tempRoot -Mode Production -MetadataDocument $metadata)
        $caseLibEntries = @($caseProduction | Where-Object { $_.Path -ieq 'tools/rogue/src/lib.rs' })
        if ($caseProduction.Count -ne $production.Count -or
            $caseLibEntries.Count -ne 1 -or
            $caseLibEntries[0].Path -cne 'tools/rogue/src/lib.rs') {
          throw 'Windows case-only Cargo target alias was not deduplicated to the stable source-enumerated repository path'
        }
      } finally {
        $metadata.packages[2].targets = $caseOriginalTargets
      }
    }
    $targetEntry = @($production | Where-Object { $_.Path -ceq 'tools/rogue/src/engine/code.txt' })
    if ($targetEntry.Count -ne 1) {
      throw 'non-.rs Cargo production target was not retained exactly once'
    }
    if ($null -ne $CodeTargetAssertion) {
      $helperEntry = @($production | Where-Object { $_.Path -ceq 'tools/rogue/src/engine/helper.rs' })
      if ($helperEntry.Count -ne 1) {
        throw 'recursive src/engine/helper.rs module was not retained exactly once'
      }
      & $CodeTargetAssertion `
        ([System.IO.FileInfo]$targetEntry[0].File) `
        ([string]$targetContent) `
        ([System.IO.FileInfo]$helperEntry[0].File) `
        ([string]$helperContent)
    }

    $layout = @(Get-RustGuardFiles -RepoRoot $tempRoot -Mode TestLayout -MetadataDocument $metadata)
    $layoutPaths = @($layout | ForEach-Object { $_.Path })
    if ($layoutPaths -cnotcontains 'tools/rogue/src/test_helpers.rs' -or
        $layoutPaths -cnotcontains 'tools/rogue/src/tests/hidden.rs' -or
        $layoutPaths -cnotcontains 'tools/rogue/src/engine/code.txt' -or
        $layoutPaths -cnotcontains 'tools/rogue/src/engine/helper.rs' -or
        $layoutPaths -ccontains 'tools/rogue/tests/ignored.rs' -or
        $layoutPaths -ccontains 'tools/rogue/benches/ignored.rs' -or
        $layoutPaths -ccontains 'tools/rogue/examples/ignored.rs') {
      throw 'test-layout boundary did not include explicit src test debt'
    }
    if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) {
      $caseOriginalTargets = @($metadata.packages[2].targets)
      try {
        $metadata.packages[2].targets = @($caseOriginalTargets) + @(
          [PSCustomObject]@{ kind = @('bin'); src_path = (Join-Path $rogueRoot 'SRC/LIB.RS') }
        )
        $caseLayout = @(Get-RustGuardFiles -RepoRoot $tempRoot -Mode TestLayout -MetadataDocument $metadata)
        $caseLayoutLibEntries = @($caseLayout | Where-Object { $_.Path -ieq 'tools/rogue/src/lib.rs' })
        if ($caseLayout.Count -ne $layout.Count -or
            $caseLayoutLibEntries.Count -ne 1 -or
            $caseLayoutLibEntries[0].Path -cne 'tools/rogue/src/lib.rs') {
          throw 'Test-layout scan retained a duplicate Windows case-only Cargo target identity'
        }
      } finally {
        $metadata.packages[2].targets = $caseOriginalTargets
      }
    }

    $originalTargets = @($metadata.packages[2].targets)
    $metadata.packages[2].targets = @($originalTargets) + @(
      [PSCustomObject]@{ kind = @('bin'); src_path = (Join-Path $rogueRoot 'src/missing-target.txt') }
    )
    $missingRejected = $false
    try {
      [void](Get-RustGuardFiles -RepoRoot $tempRoot -Mode Production -MetadataDocument $metadata)
    } catch {
      $missingRejected = $_.Exception.Message -match 'src_path is not an existing file'
    }
    $metadata.packages[2].targets = $originalTargets
    if (-not $missingRejected) {
      throw 'cargo metadata scanner accepted a missing production target'
    }

    $outsideTarget = Join-Path ([System.IO.Path]::GetTempPath()) ('meow-outside-target-' + [guid]::NewGuid().ToString('N') + '.txt')
    [System.IO.File]::WriteAllText($outsideTarget, 'fn outside() {}', $Encoding)
    $packageRootTarget = Join-Path $rogueRoot 'outside-src.rs'
    [System.IO.File]::WriteAllText($packageRootTarget, 'fn package_root() {}', $Encoding)
    $sharedRoot = Join-Path $tempRoot 'tools/shared'
    [void][System.IO.Directory]::CreateDirectory($sharedRoot)
    $sharedTarget = Join-Path $sharedRoot 'shared.rs'
    [System.IO.File]::WriteAllText($sharedTarget, 'fn shared() {}', $Encoding)
    $invalidTargets = @(
      [PSCustomObject]@{
        Name = 'missing src_path property'
        Target = [PSCustomObject]@{ kind = @('lib') }
        Pattern = 'missing src_path'
      },
      [PSCustomObject]@{
        Name = 'empty src_path'
        Target = [PSCustomObject]@{ kind = @('lib'); src_path = '' }
        Pattern = 'missing src_path'
      },
      [PSCustomObject]@{
        Name = 'directory src_path'
        Target = [PSCustomObject]@{ kind = @('lib'); src_path = $engineRoot }
        Pattern = 'not an existing file'
      },
      [PSCustomObject]@{
        Name = 'repository-external src_path'
        Target = [PSCustomObject]@{ kind = @('lib'); src_path = $outsideTarget }
        Pattern = 'must be inside its owning package src directory'
      },
      [PSCustomObject]@{
        Name = 'package-root production target'
        Target = [PSCustomObject]@{ kind = @('bin'); src_path = $packageRootTarget }
        Pattern = 'must be inside its owning package src directory'
      },
      [PSCustomObject]@{
        Name = '../shared production target'
        Target = [PSCustomObject]@{ kind = @('bin'); src_path = $sharedTarget }
        Pattern = 'must be inside its owning package src directory'
      },
      [PSCustomObject]@{
        Name = 'cross-package production target'
        Target = [PSCustomObject]@{ kind = @('bin'); src_path = (Join-Path $normalRoot 'src/lib.rs') }
        Pattern = 'must be inside its owning package src directory'
      }
    )
    foreach ($invalidTarget in $invalidTargets) {
      $metadata.packages[2].targets = @($originalTargets) + @($invalidTarget.Target)
      $invalidRejected = $false
      try {
        [void](Get-RustGuardFiles -RepoRoot $tempRoot -Mode Production -MetadataDocument $metadata)
      } catch {
        $invalidRejected = $_.Exception.Message -match $invalidTarget.Pattern
      }
      if (-not $invalidRejected) {
        throw "cargo metadata scanner accepted invalid production target: $($invalidTarget.Name)"
      }
    }
    $metadata.packages[2].targets = $originalTargets
    $postInvalidPaths = @(
      Get-RustGuardFiles -RepoRoot $tempRoot -Mode Production -MetadataDocument $metadata |
        ForEach-Object { $_.Path }
    )
    if ($postInvalidPaths -ccontains 'tools/rogue/outside-src.rs') {
      throw 'Rust workspace scanner retained a non-target package-root sibling .rs file'
    }

    $realTargetRoot = Join-Path $engineRoot 'real-target'
    $linkedTargetRoot = Join-Path $engineRoot 'linked-target'
    [void][System.IO.Directory]::CreateDirectory($realTargetRoot)
    $realTarget = Join-Path $realTargetRoot 'linked-code.txt'
    [System.IO.File]::WriteAllText($realTarget, 'fn linked() {}', $Encoding)
    $junctionCreated = $false
    try {
      [void](New-Item -ItemType Junction -Path $linkedTargetRoot -Target $realTargetRoot -ErrorAction Stop)
      $junctionCreated = $true
    } catch {
      $junctionCreated = $false
    }
    if ($junctionCreated) {
      $metadata.packages[2].targets = @($originalTargets) + @(
        [PSCustomObject]@{ kind = @('lib'); src_path = (Join-Path $linkedTargetRoot 'linked-code.txt') }
      )
      $reparseRejected = $false
      try {
        [void](Get-RustGuardFiles -RepoRoot $tempRoot -Mode Production -MetadataDocument $metadata)
      } catch {
        $reparseRejected = $_.Exception.Message -match 'crosses a reparse point'
      }
      $metadata.packages[2].targets = $originalTargets
      if (-not $reparseRejected) {
        throw 'cargo metadata scanner accepted a production target through a junction'
      }
      if (Test-Path -LiteralPath $linkedTargetRoot) {
        [System.IO.Directory]::Delete($linkedTargetRoot, $false)
      }
    } elseif (-not (Test-RustGuardPathContainsReparsePoint -RootPath $tempRoot -TargetPath (Join-Path $engineRoot 'missing/component.txt'))) {
      throw 'production target reparse validator did not fail closed when junction creation was unavailable'
    }

    $nestedRoot = Join-Path $rogueRoot 'src/nested-member'
    [void][System.IO.Directory]::CreateDirectory((Join-Path $nestedRoot 'src'))
    [System.IO.File]::WriteAllText((Join-Path $nestedRoot 'Cargo.toml'), "[package]`nname = `"nested`"`nversion = `"0.0.0`"`n", $Encoding)
    [System.IO.File]::WriteAllText((Join-Path $nestedRoot 'src/lib.rs'), 'fn nested() {}', $Encoding)
    $nestedId = 'nested-id'
    $originalMembers = @($metadata.workspace_members)
    $originalPackages = @($metadata.packages)
    try {
      $metadata.workspace_members = @($originalMembers) + @($nestedId)
      $metadata.packages = @($originalPackages) + @(
        [PSCustomObject]@{
          id = $nestedId
          name = 'nested'
          manifest_path = (Join-Path $nestedRoot 'Cargo.toml')
          targets = @([PSCustomObject]@{ kind = @('lib'); src_path = (Join-Path $nestedRoot 'src/lib.rs') })
        }
      )
      $nestedRejected = $false
      try {
        [void](Get-RustGuardFiles -RepoRoot $tempRoot -Mode Production -MetadataDocument $metadata)
      } catch {
        $nestedRejected = $_.Exception.Message -match 'unsupported nested workspace member layout.*inside.*source directory'
      }
      if (-not $nestedRejected) {
        throw 'Rust workspace scanner accepted a workspace member nested under another member src directory'
      }
    } finally {
      $metadata.workspace_members = $originalMembers
      $metadata.packages = $originalPackages
      if (Test-Path -LiteralPath $nestedRoot) {
        Remove-Item -LiteralPath $nestedRoot -Recurse -Force
      }
    }

    $rogueLib = Join-Path $rogueRoot 'src/lib.rs'
    $hiddenRoot = Join-Path $rogueRoot 'src/hidden'
    [void][System.IO.Directory]::CreateDirectory($hiddenRoot)
    [System.IO.File]::WriteAllText((Join-Path $hiddenRoot 'code.txt'), 'fn hidden() {}', $Encoding)
    $pathModuleSource = @'
#[cfg(test)]
#[path = "hidden/code.txt"]
mod hidden;
'@
    [System.IO.File]::WriteAllText($rogueLib, $pathModuleSource, $Encoding)
    $pathModuleRejected = $false
    $pathModuleError = $null
    try {
      [void](Get-RustGuardFiles -RepoRoot $tempRoot -Mode Production -MetadataDocument $metadata)
    } catch {
      $pathModuleError = $_.Exception.Message
      $pathModuleRejected = $pathModuleError -match '#\[path\]'
    }
    if (-not $pathModuleRejected) {
      throw "production #[path] non-.rs module injection was accepted: $pathModuleError"
    }

    $includeSource = @'
fn injected() { include!("hidden.txt"); }
'@
    [System.IO.File]::WriteAllText($rogueLib, $includeSource, $Encoding)
    $includeRejected = $false
    try {
      [void](Get-RustGuardFiles -RepoRoot $tempRoot -Mode Production -MetadataDocument $metadata)
    } catch {
      $includeRejected = $_.Exception.Message -match 'include! source injection is prohibited'
    }
    if (-not $includeRejected) {
      throw 'production include! source injection was accepted'
    }

    $allowedIncludes = @'
const TEXT: &str = include_str!("hidden.txt");
const BYTES: &[u8] = include_bytes!("hidden.txt");
'@
    [System.IO.File]::WriteAllText($rogueLib, $allowedIncludes, $Encoding)
    [void](Get-RustGuardFiles -RepoRoot $tempRoot -Mode Production -MetadataDocument $metadata)
  } finally {
    if ($null -ne $outsideTarget -and (Test-Path -LiteralPath $outsideTarget)) {
      Remove-Item -LiteralPath $outsideTarget -Force
    }
    if (Test-Path -LiteralPath $tempRoot) {
      Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
  }
}
