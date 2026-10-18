$ErrorActionPreference = 'Stop'
Import-Module (Join-Path (Split-Path -Parent $PSScriptRoot) 'GpOracle.psm1') -Force -DisableNameChecking -WarningAction SilentlyContinue

$script:Failures = New-Object System.Collections.ArrayList
$script:Passed = 0

function Assert-True([bool]$Condition, [string]$Name) {
  if ($Condition) { $script:Passed++ } else { [void]$script:Failures.Add($Name) }
}

function Assert-Throws([scriptblock]$Body, [string]$Name) {
  $threw = $false
  try { & $Body | Out-Null } catch { $threw = $true }
  Assert-True $threw $Name
}

function New-FakeWindow([int64]$Handle, [int64]$Owner, [string]$Title, [string]$Class) {
  [pscustomobject]@{ Handle = [IntPtr]$Handle; Owner = [IntPtr]$Owner; Title = $Title; Class = $Class }
}

$main = [IntPtr]100
$zero = [IntPtr]::Zero
$windows = @(
  (New-FakeWindow 100 0 'song - Guitar Pro 8' 'Qt5153QWindowIcon'),
  (New-FakeWindow 200 100 'About Guitar Pro 8' 'Qt5153QWindowIcon'),
  (New-FakeWindow 210 100 'Open Files' '#32770'),
  (New-FakeWindow 300 0 'Mixer - Guitar Pro 8' 'Qt5153QWindowIcon'),
  (New-FakeWindow 310 0 'Floating panel' 'Qt5153QWindowToolSaveBits'),
  (New-FakeWindow 400 100 '' 'SysShadow'),
  (New-FakeWindow 410 100 'tip' 'Qt5153QWindowToolTipSaveBits'),
  (New-FakeWindow 500 100 'Guitar Pro 8' 'Qt5153QWindowPopupDropShadowSaveBits'),
  (New-FakeWindow 600 999 'Other owner' 'Qt5153QWindowIcon')
)

Assert-Throws { Select-GpModals $windows $zero } 'Select-GpModals refuses an unknown main window'
Assert-Throws { Assert-MainKnown $zero } 'Assert-MainKnown throws on zero'

$selected = @(Select-GpModals $windows $main | ForEach-Object { [int64]$_.Handle })
Assert-True (($selected -join ',') -eq '200,210') "Select-GpModals keeps only windows owned by main, got [$($selected -join ',')]"
Assert-True ($selected -notcontains 100) 'main window is never a modal'
Assert-True ($selected -notcontains 300 -and $selected -notcontains 310) 'unowned top-level windows are never modals'

Assert-True (Test-CloseAllowed ([IntPtr]200) $main $main) 'owned dialog may be closed'
Assert-True (-not (Test-CloseAllowed $main $zero $main)) 'main window may not be closed'
Assert-True (-not (Test-CloseAllowed $main $main $main)) 'main window may not be closed even if it reports itself as owner'
Assert-True (-not (Test-CloseAllowed ([IntPtr]300) $zero $main)) 'unowned window may not be closed'
Assert-True (-not (Test-CloseAllowed ([IntPtr]600) ([IntPtr]999) $main)) 'window owned by another window may not be closed'
Assert-True (-not (Test-CloseAllowed ([IntPtr]200) $main $zero)) 'nothing may be closed while main is unknown'
Assert-True (-not (Test-CloseAllowed $zero $zero $zero)) 'zero handle may not be closed'
Assert-True (-not (Test-CloseAllowed ([IntPtr]300) $zero $zero)) 'an unowned window may not be closed while main is unknown'

Assert-True ((ConvertTo-LabelKey '&Save As...') -eq 'save as') 'label key strips accelerator and ellipsis'
Assert-True ((ConvertTo-LabelKey ('Export' + [char]0x2026)) -eq 'export') 'label key strips unicode ellipsis'

if ($script:Failures.Count -gt 0) {
  foreach ($f in $script:Failures) { Write-Output "FAIL: $f" }
  [Environment]::Exit(1)
}
Write-Output "PASS: $($script:Passed) assertions"
[Environment]::Exit(0)
