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
  New-GpWindowInfo ([IntPtr]$Handle) $Title $Class ([IntPtr]$Owner)
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
  (New-FakeWindow 600 999 'Other owner' 'Qt5153QWindowIcon'),
  (New-FakeWindow 700 100 'Owned tool panel' 'Qt5153QWindowToolSaveBits'),
  (New-FakeWindow 710 100 'Owned tool' 'Qt5153QWindowTool'),
  (New-FakeWindow 720 100 'Owned unknown' 'SomeOtherClass')
)

Assert-Throws { Select-GpModals $windows $zero } 'Select-GpModals refuses an unknown main window'
Assert-Throws { Assert-MainKnown $zero } 'Assert-MainKnown throws on zero'

$selected = @(Select-GpModals $windows $main | ForEach-Object { [int64]$_.Handle })
Assert-True (($selected -join ',') -eq '200,210') "Select-GpModals keeps only windows owned by main, got [$($selected -join ',')]"
Assert-True ($selected -notcontains 100) 'main window is never a modal'
Assert-True ($selected -notcontains 300 -and $selected -notcontains 310) 'unowned top-level windows are never modals'
Assert-True ($selected -notcontains 700 -and $selected -notcontains 710) 'owned Qt tool panels are never modals'
Assert-True ($selected -notcontains 720) 'owned windows of unknown classes are never modals'
Assert-True ($selected -notcontains 400 -and $selected -notcontains 410 -and $selected -notcontains 500) 'shadows, tooltips and popups are never modals'

Assert-True (Test-ModalClass 'Qt5153QWindowIcon') 'Qt dialog class is a modal class'
Assert-True (Test-ModalClass '#32770') 'native dialog class is a modal class'
foreach ($tool in @('Qt5153QWindowToolSaveBits', 'Qt5153QWindowTool', 'Qt5153QWindowToolTipSaveBits', 'Qt5153QWindowPopupDropShadowSaveBits', 'SysShadow', 'Edit', '')) {
  Assert-True (-not (Test-ModalClass $tool)) "class '$tool' is not a modal class"
}

$popups = @(Select-PopupWindows $windows | ForEach-Object { [int64]$_.Handle })
Assert-True (($popups -join ',') -eq '500') "Select-PopupWindows keeps only popup classes, got [$($popups -join ',')]"
Assert-True (@(Select-PopupWindows @()).Count -eq 0) 'no windows means no popups'

$script:Posted = New-Object System.Collections.ArrayList
$owners = @{ 200 = 100; 210 = 100; 300 = 0; 600 = 999; 100 = 0; 700 = 100 }
Set-GpWindowBackend -OwnerProbe { param($h) [IntPtr][int64]($owners[[int]$h]) } -Closer { param($h, $m) [void]$script:Posted.Add("$([int]$h):$([int]$m)") }
try {
  Assert-True (Close-OwnedWindow ([IntPtr]200) $main) 'Close-OwnedWindow closes an owned dialog'
  Assert-True (Close-OwnedWindow ([IntPtr]210) $main) 'Close-OwnedWindow closes an owned native dialog'
  Assert-True (-not (Close-OwnedWindow $main $main)) 'Close-OwnedWindow refuses the main window'
  Assert-True (-not (Close-OwnedWindow ([IntPtr]300) $main)) 'Close-OwnedWindow refuses an unowned window'
  Assert-True (-not (Close-OwnedWindow ([IntPtr]600) $main)) 'Close-OwnedWindow refuses a window owned by another window'
  Assert-True (-not (Close-OwnedWindow $zero $main)) 'Close-OwnedWindow refuses a zero handle'
  Assert-Throws { Close-OwnedWindow ([IntPtr]200) $zero } 'Close-OwnedWindow throws while main is unknown'
  Assert-True (($script:Posted -join ',') -eq '200:16,210:16') "only the owned dialogs received WM_CLOSE, got [$($script:Posted -join ',')]"
} finally {
  Reset-GpWindowBackend
}

$step = New-GpStepRecord 'pdf' 'ok'
$step.error = 'x'
$json = $step | ConvertTo-Json | ConvertFrom-Json
Assert-True (($json.PSObject.Properties.Name -join ',') -eq 'name,status,ms,attempts,error,screenshot') 'step record serialises with the lowercase keys the harness reads'
Assert-True ($json.attempts -eq 1 -and $json.status -eq 'ok' -and $null -eq $json.screenshot) 'step record defaults'

Assert-True (Test-JobFolder 'C:\gf-oracle\jobs\123-4-000-0') 'a job folder is accepted'
Assert-True (Test-JobFolder 'c:\GF-ORACLE\Jobs\123\out\') 'job folder matching ignores case and trailing separators'
foreach ($bad in @('C:\gf-oracle\jobs', 'C:\gf-oracle\jobs\', 'C:\gf-oracle\jobsX\1', 'C:\gf-oracle\jobs\..\other', 'C:\gf-oracle\jobs\..\..\Windows', 'C:\Users\x', 'C:\', 'D:\gf-oracle\jobs\1', '\\host\share\gf-oracle\jobs\1', '', ' ')) {
  Assert-True (-not (Test-JobFolder $bad)) "folder '$bad' is not under the jobs root"
  Assert-True ((Close-ExplorerFolder $bad 0) -eq 0) "Close-ExplorerFolder refuses '$bad'"
}

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

Assert-True (-not [GpOracle.W32]::Focus([IntPtr]::Zero)) 'Focus refuses a null handle'
Assert-True (-not [GpOracle.W32]::Focus([IntPtr]0x7FFFFFF0)) 'Focus refuses a handle that is not a window'

if ($script:Failures.Count -gt 0) {
  foreach ($f in $script:Failures) { Write-Output "FAIL: $f" }
  [Environment]::Exit(1)
}
Write-Output "PASS: $($script:Passed) assertions"
[Environment]::Exit(0)
