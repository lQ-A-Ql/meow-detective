# Requires -Version 5.1
<#
.SYNOPSIS
  Generates deterministic public-medium ISO9660/Joliet and flat-VMDK fixtures.
.DESCRIPTION
  The generated images contain only synthetic, non-sensitive bytes. The ISO
  extent is reused as the flat VMDK backing file so the VMDK fixture also
  exercises the ISO-on-VMDK composition path.
#>
param(
  [string]$OutputRoot = (Join-Path (Split-Path -Parent $PSScriptRoot) 'testdata/fixtures/public-medium')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$blockSize = 2048
$blockCount = 256
$isoLength = $blockSize * $blockCount
$isoDir = Join-Path $OutputRoot 'iso'
$vmdkDir = Join-Path $OutputRoot 'vmdk'
[System.IO.Directory]::CreateDirectory($isoDir) | Out-Null
[System.IO.Directory]::CreateDirectory($vmdkDir) | Out-Null

function Set-BothEndianU32 {
  param(
    [Parameter(Mandatory = $true)][byte[]]$Buffer,
    [Parameter(Mandatory = $true)][int]$Offset,
    [Parameter(Mandatory = $true)][uint32]$Value
  )

  $Buffer[$Offset] = [byte]($Value -band 0xff)
  $Buffer[$Offset + 1] = [byte](($Value -shr 8) -band 0xff)
  $Buffer[$Offset + 2] = [byte](($Value -shr 16) -band 0xff)
  $Buffer[$Offset + 3] = [byte](($Value -shr 24) -band 0xff)
  $Buffer[$Offset + 4] = [byte](($Value -shr 24) -band 0xff)
  $Buffer[$Offset + 5] = [byte](($Value -shr 16) -band 0xff)
  $Buffer[$Offset + 6] = [byte](($Value -shr 8) -band 0xff)
  $Buffer[$Offset + 7] = [byte]($Value -band 0xff)
}

function New-DirectoryRecord {
  param(
    [Parameter(Mandatory = $true)][uint32]$Extent,
    [Parameter(Mandatory = $true)][uint32]$Size,
    [Parameter(Mandatory = $true)][byte]$Flags,
    [Parameter(Mandatory = $true)][byte[]]$Name
  )

  $padding = if (($Name.Length % 2) -eq 0) { 1 } else { 0 }
  $record = New-Object byte[] (33 + $Name.Length + $padding)
  $record[0] = [byte]$record.Length
  Set-BothEndianU32 -Buffer $record -Offset 2 -Value $Extent
  Set-BothEndianU32 -Buffer $record -Offset 10 -Value $Size
  $record[25] = $Flags
  $record[28] = 1
  $record[30] = 0
  $record[31] = 1
  $record[32] = [byte]$Name.Length
  [Array]::Copy($Name, 0, $record, 33, $Name.Length)
  return $record
}

function Convert-ToJolietName {
  param([Parameter(Mandatory = $true)][string]$Name)
  $utf16 = [System.Text.Encoding]::BigEndianUnicode.GetBytes($Name)
  return [byte[]]$utf16
}

function Add-Record {
  param(
    [Parameter(Mandatory = $true)][byte[]]$Directory,
    [Parameter(Mandatory = $true)][int]$Offset,
    [Parameter(Mandatory = $true)][byte[]]$Record
  )
  [Array]::Copy($Record, 0, $Directory, $Offset, $Record.Length)
  return $Offset + $Record.Length
}

function New-DirectoryBlock {
  param(
    [Parameter(Mandatory = $true)][uint32]$Extent,
    [Parameter(Mandatory = $true)][uint32]$ParentExtent,
    [Parameter(Mandatory = $true)][object[]]$Entries,
    [Parameter(Mandatory = $true)][bool]$Joliet
  )

  $directory = New-Object byte[] $blockSize
  $offset = 0
  $selfName = [byte[]](0)
  $parentName = [byte[]](1)
  $offset = Add-Record -Directory $directory -Offset $offset -Record (
    New-DirectoryRecord -Extent $Extent -Size $blockSize -Flags 0x02 -Name $selfName
  )
  $offset = Add-Record -Directory $directory -Offset $offset -Record (
    New-DirectoryRecord -Extent $ParentExtent -Size $blockSize -Flags 0x02 -Name $parentName
  )
  foreach ($entry in $Entries) {
    $name = if ($Joliet) {
      Convert-ToJolietName -Name $entry.Name
    } else {
      [System.Text.Encoding]::ASCII.GetBytes($entry.Name)
    }
    $offset = Add-Record -Directory $directory -Offset $offset -Record (
      New-DirectoryRecord -Extent $entry.Extent -Size $entry.Size -Flags $entry.Flags -Name $name
    )
  }
  return $directory
}

function Set-Descriptor {
  param(
    [Parameter(Mandatory = $true)][byte[]]$Image,
    [Parameter(Mandatory = $true)][int]$Block,
    [Parameter(Mandatory = $true)][byte]$Type,
    [Parameter(Mandatory = $true)][uint32]$RootExtent
  )

  $descriptor = New-Object byte[] $blockSize
  $descriptor[0] = $Type
  [Array]::Copy([System.Text.Encoding]::ASCII.GetBytes('CD001'), 0, $descriptor, 1, 5)
  $descriptor[6] = 1
  Set-BothEndianU32 -Buffer $descriptor -Offset 80 -Value $blockCount
  $descriptor[128] = 0
  $descriptor[129] = 8
  $descriptor[130] = 8
  $descriptor[131] = 0
  $root = New-DirectoryRecord -Extent $RootExtent -Size $blockSize -Flags 0x02 -Name ([byte[]](0))
  [Array]::Copy($root, 0, $descriptor, 156, $root.Length)
  if ($Type -eq 2) {
    [Array]::Copy([System.Text.Encoding]::ASCII.GetBytes('%/E'), 0, $descriptor, 88, 3)
  }
  [Array]::Copy($descriptor, 0, $Image, $Block * $blockSize, $descriptor.Length)
}

$image = New-Object byte[] $isoLength
Set-Descriptor -Image $image -Block 16 -Type 1 -RootExtent 20
Set-Descriptor -Image $image -Block 17 -Type 2 -RootExtent 20
$terminator = New-Object byte[] $blockSize
$terminator[0] = 255
[Array]::Copy([System.Text.Encoding]::ASCII.GetBytes('CD001'), 0, $terminator, 1, 5)
$terminator[6] = 1
[Array]::Copy($terminator, 0, $image, 18 * $blockSize, $terminator.Length)

$rootEntries = @(
  [pscustomobject]@{ Name = 'README.TXT;1'; Extent = 30; Size = 8192; Flags = 0 },
  [pscustomobject]@{ Name = 'REPORT'; Extent = 21; Size = $blockSize; Flags = 2 },
  [pscustomobject]@{
    Name = (([char]0x62A5).ToString() + ([char]0x544A).ToString() + '.TXT;1')
    Extent = 34
    Size = 4096
    Flags = 0
  }
)
$reportEntries = @(
  [pscustomobject]@{ Name = 'SUMMARY.TXT;1'; Extent = 31; Size = 4096; Flags = 0 },
  [pscustomobject]@{ Name = 'DATA'; Extent = 22; Size = $blockSize; Flags = 2 }
)
$dataEntries = @(
  [pscustomobject]@{ Name = 'VALUES.BIN;1'; Extent = 32; Size = 2048; Flags = 0 }
)
foreach ($joliet in @($false, $true)) {
  $root = New-DirectoryBlock -Extent 20 -ParentExtent 20 -Entries $rootEntries -Joliet $joliet
  $report = New-DirectoryBlock -Extent 21 -ParentExtent 20 -Entries $reportEntries -Joliet $joliet
  $data = New-DirectoryBlock -Extent 22 -ParentExtent 21 -Entries $dataEntries -Joliet $joliet
  [Array]::Copy($root, 0, $image, 20 * $blockSize, $root.Length)
  [Array]::Copy($report, 0, $image, 21 * $blockSize, $report.Length)
  [Array]::Copy($data, 0, $image, 22 * $blockSize, $data.Length)
}

$readme = [System.Text.Encoding]::UTF8.GetBytes(("Public medium ISO fixture`n" * 300))
$summary = [System.Text.Encoding]::UTF8.GetBytes(("Nested report fixture`n" * 180))
$values = New-Object byte[] 2048
for ($index = 0; $index -lt $values.Length; $index++) {
  $values[$index] = [byte]($index % 256)
}
$reportLabel = ([char]0x62A5).ToString() + ([char]0x544A).ToString()
$reportText = [System.Text.Encoding]::UTF8.GetBytes(("$reportLabel fixture`n" * 220))
[Array]::Copy($readme, 0, $image, 30 * $blockSize, $readme.Length)
[Array]::Copy($summary, 0, $image, 31 * $blockSize, $summary.Length)
[Array]::Copy($values, 0, $image, 32 * $blockSize, $values.Length)
[Array]::Copy($reportText, 0, $image, 34 * $blockSize, $reportText.Length)

$isoPath = Join-Path $isoDir 'medium.iso'
[System.IO.File]::WriteAllBytes($isoPath, $image)

$extentPath = Join-Path $vmdkDir 'medium-flat.bin'
[System.IO.File]::WriteAllBytes($extentPath, $image)
$descriptor = @(
  '# Disk DescriptorFile'
  'version=1'
  'CID=12345678'
  'parentCID=ffffffff'
  'createType="monolithicFlat"'
  ('RW {0} FLAT "medium-flat.bin" 0' -f ($isoLength / 512))
) -join "`n"
[System.IO.File]::WriteAllText(
  (Join-Path $vmdkDir 'medium-flat.vmdk'),
  "$descriptor`n",
  [System.Text.UTF8Encoding]::new($false)
)
Write-Host "Generated $isoPath, $extentPath and medium-flat.vmdk ($isoLength bytes)"
