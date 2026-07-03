$data = Get-Content 'D:\process\forensic\.understand-anything\intermediate\batches.json' -Raw | ConvertFrom-Json
$batch4 = $data.batches[4]
$batch4 | ConvertTo-Json -Depth 100 | Out-File -Encoding utf8 'D:\process\forensic\.understand-anything\tmp\batch-4-extracted.json'
Write-Output 'Done'
