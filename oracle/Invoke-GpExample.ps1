param(
  [Parameter(Mandatory = $true)][string]$OutDir,
  [string]$Example = '',
  [string]$Target = '',
  [ValidateSet('list', 'save')][string]$Mode = 'save',
  [int]$DialogTimeoutMs = 8000,
  [int]$StepTimeoutMs = 60000,
  [int]$MaxAttempts = 3
)

$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'GpOracle.psm1') -Force -DisableNameChecking -WarningAction SilentlyContinue

$ExampleMenu = @('File', 'Open Example')
$ExamplePrefix = 'FileFeature.actionNew'
$Menus = @{
  SaveAs = @('File', 'Save As...|Save as')
  Close  = @('File', 'Close')
}
$GpSuffix = ' - Guitar Pro 8'
$EmptyTitle = "Empty$GpSuffix"

$Diag = Join-Path $OutDir 'diagnostics'
New-Item -ItemType Directory -Force -Path $Diag | Out-Null
$PidTemp = Join-Path $OutDir 'pid.tmp'
Write-Utf8File $PidTemp ([string]$PID + "`n" + [string](Get-Process -Id $PID).StartTime.ToFileTimeUtc() + "`n")
Move-Item -LiteralPath $PidTemp -Destination (Join-Path $OutDir 'pid.txt') -Force

$Steps = New-Object System.Collections.ArrayList
$ModalsAtStart = New-Object System.Collections.ArrayList
$Notes = New-Object System.Collections.ArrayList
$Examples = New-Object System.Collections.ArrayList
$script:LastMenuError = $null
$List = $Mode -eq 'list'
$Result = [ordered]@{
  ok              = $false
  gp_running      = $false
  gp_version      = $null
  gp_pid          = $null
  mode            = $Mode
  example         = $Example
  target          = $Target
  error           = $null
  examples        = $Examples
  modals_at_start = $ModalsAtStart
  notes           = $Notes
  steps           = $Steps
}
$script:Attempts = 1

function Write-Result {
  Write-Utf8File (Join-Path $OutDir 'result.json') ($Result | ConvertTo-Json -Depth 6)
}

function Invoke-Step([string]$Name, [scriptblock]$Body) {
  $rec = New-GpStepRecord $Name 'ok'
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  $script:Attempts = 1
  try {
    & $Body | Out-Null
  } catch {
    $rec.status = 'failed'
    $rec.error = $_.Exception.Message
    $rec.screenshot = Save-Screenshot (Join-Path $Diag "fail-$Name.png")
    try { Write-Utf8File (Join-Path $Diag "fail-$Name.txt") ((Get-ModalDump) -join "`r`n") } catch { }
    try { [void](Clear-GpModals $MaxAttempts) } catch { $rec.error = "$($rec.error); cleanup failed: $($_.Exception.Message)" }
  }
  $rec.ms = [int64]$sw.ElapsedMilliseconds
  $rec.attempts = $script:Attempts
  [void]$Steps.Add($rec)
  [void](Close-MenuRunspaces)
  $rec.status -eq 'ok'
}

function Skip-Step([string]$Name, [string]$Why) {
  $rec = New-GpStepRecord $Name 'skipped'
  $rec.attempts = 0
  $rec.error = $Why
  [void]$Steps.Add($rec)
}

function Get-ModalTitles { (@(Get-GpModals) | ForEach-Object { "'$($_.Title)' [$($_.Class)]" }) -join ', ' }

function Test-FileReady([string]$Path) {
  try {
    $s = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::None)
    $s.Dispose()
    $true
  } catch { $false }
}

function Get-ExampleNames {
  $type = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::MenuItem)
  $names = New-Object System.Collections.ArrayList
  try {
    $sub = Open-MenuPath $ExampleMenu
    [void](Expand-El $sub)
    $found = Wait-For {
      foreach ($pp in Get-GpPopups) {
        $hits = @($pp.FindAll([System.Windows.Automation.TreeScope]::Descendants, $type) | Where-Object { $_.Current.AutomationId.StartsWith($ExamplePrefix) })
        if ($hits.Count -gt 0) { return , $hits }
      }
    } $DialogTimeoutMs
    if (-not $found) { throw "no '$ExamplePrefix' items under $($ExampleMenu -join ' > ')" }
    foreach ($x in $found) {
      $n = $x.Current.AutomationId.Substring($ExamplePrefix.Length)
      if (-not $names.Contains($n)) { [void]$names.Add($n) }
    }
  } finally {
    if (-not (Close-GpMenus)) { [void]$Notes.Add('menus did not close after listing') }
  }
  $names
}

function Open-Example([string]$Name) {
  $before = Get-GpTitle
  $path = $ExampleMenu + @("aid:$ExamplePrefix$Name|$Name")
  for ($a = 1; $a -le $MaxAttempts; $a++) {
    $script:Attempts = $a
    $c = Clear-GpModals $MaxAttempts
    if (-not $c.Cleared) { throw "cannot clear dialogs before opening '$Name': $($c.Seen -join ', ')" }
    try {
      [void](Invoke-MenuAsync $path)
    } catch {
      $script:LastMenuError = $_.Exception.Message
      [void](Close-GpMenus)
      continue
    }
    $t = Wait-For { $now = Get-GpTitle; if ($now -ne $before -and $now.EndsWith($GpSuffix)) { $now } } $StepTimeoutMs 200
    if ($t) { return $t }
    $toast = Close-GpErrorToast
    if ($toast) { throw "Guitar Pro reported: $toast" }
    [void](Close-GpMenus)
  }
  throw "example '$Name' did not open after $MaxAttempts attempts; last menu error: $script:LastMenuError"
}

function Open-SaveDialog {
  for ($a = 1; $a -le $MaxAttempts; $a++) {
    $script:Attempts = $a
    $c = Clear-GpModals $MaxAttempts
    if (-not $c.Cleared) { throw "cannot clear dialogs before Save As: $($c.Seen -join ', ')" }
    $before = @(Get-ProcWindows (Get-GpProcess).Id)
    try {
      [void](Invoke-MenuAsync $Menus.SaveAs)
    } catch {
      $script:LastMenuError = $_.Exception.Message
      [void](Close-GpMenus)
      continue
    }
    $w = Wait-NewWindow $before $DialogTimeoutMs
    if ($w) {
      if ($w.Class -ne '#32770') { throw "expected the Save As file dialog, got '$($w.Title)' [$($w.Class)]" }
      return $w
    }
    [void](Close-GpMenus)
  }
  throw "Save As dialog never appeared after $MaxAttempts attempts; last menu error: $script:LastMenuError"
}

function Save-Example([string]$Path) {
  if (Test-Path -LiteralPath $Path) { Remove-Item -LiteralPath $Path -Force }
  $w = Open-SaveDialog
  Complete-FileDialog $w.Handle $Path
  $gone = Wait-For { if (-not [GpOracle.W32]::IsWindow($w.Handle)) { $true } } 10000
  if (-not $gone) { throw "'$($w.Title)' did not close" }
  $script:lastLen = -1
  $done = Wait-For {
    if (Test-Path -LiteralPath $Path) {
      $l = (Get-Item -LiteralPath $Path).Length
      if ($l -gt 0 -and $l -eq $script:lastLen -and (Test-FileReady $Path)) { return $true }
      $script:lastLen = $l
    }
    $null
  } $StepTimeoutMs 300
  if (-not $done) { throw "no stable output at $Path; open dialogs: $(Get-ModalTitles)" }
  $t = "$([System.IO.Path]::GetFileNameWithoutExtension($Path))$GpSuffix"
  if (-not (Wait-For { if ((Get-GpTitle) -eq $t) { $true } } 5000)) { throw "title did not switch to '$t'" }
  $t
}

$proc = Get-GpProcess
if (-not $proc) {
  $Result.error = 'Guitar Pro is not running. Start Guitar Pro 8 yourself and leave it on an empty document; the harness never launches it.'
  Write-Result
  exit 2
}
$Result.gp_running = $true
$Result.gp_pid = $proc.Id
$Result.gp_version = Get-GpVersion $proc

$script:openedTitle = $null

try {
  $ready = Invoke-Step 'precheck' {
    if (-not $List -and ([string]::IsNullOrWhiteSpace($Example) -or [string]::IsNullOrWhiteSpace($Target))) { throw 'save mode needs both -Example and -Target' }
    if ((Get-GpMainHandle) -eq [IntPtr]::Zero) { throw 'Guitar Pro main window not found' }
    $c = Clear-GpModals $MaxAttempts
    foreach ($s in $c.Seen) { [void]$ModalsAtStart.Add($s) }
    if (-not $c.Cleared) { throw "dialogs still open after $MaxAttempts attempts: $($c.Seen -join ', ')" }
    if (-not (Get-GpMain)) { throw 'Guitar Pro main window not reachable through UI Automation' }
    if (-not $List -and (Get-GpTitle) -ne $EmptyTitle) { throw "Guitar Pro is on '$(Get-GpTitle)', not '$EmptyTitle'; leave it on the empty document" }
  }

  if ($List) {
    if ($ready) {
      [void](Invoke-Step 'list' { foreach ($n in Get-ExampleNames) { [void]$Examples.Add($n) } })
    } else {
      Skip-Step 'list' 'precheck failed'
    }
  } else {
    $opened = $false
    if ($ready) {
      $opened = Invoke-Step 'open' { $script:openedTitle = Open-Example $Example }
    } else {
      Skip-Step 'open' 'precheck failed'
    }
    if ($opened) {
      [void](Invoke-Step 'saveas' { $script:openedTitle = Save-Example $Target })
    } else {
      Skip-Step 'saveas' 'example not open'
    }
  }
} finally {
  if ($script:openedTitle) {
    [void](Invoke-Step 'close' {
        $c = Clear-GpModals $MaxAttempts
        if (-not $c.Cleared) { throw "cannot clear dialogs before close: $($c.Seen -join ', ')" }
        $before = Get-GpTitle
        if ($before -ne $script:openedTitle) { throw "active document is '$before', not '$script:openedTitle'; left open" }
        [void](Invoke-MenuAsync $Menus.Close)
        $closed = Wait-For { $t = Get-GpTitle; if ($t -ne $before -and $t -ne '') { $true } } $StepTimeoutMs 200
        if (-not $closed) { throw "document did not close; open dialogs: $(Get-ModalTitles)" }
      })
  } elseif (-not $List) {
    Skip-Step 'close' 'example not open'
  }
  try { [void](Close-GpMenus) } catch { }
  $Result.pending_menu_invokes = Close-MenuRunspaces
  $Result.ok = (@($Steps | Where-Object { $_.status -ne 'ok' }).Count -eq 0)
  Write-Result
}

if ($Result.ok) { [Environment]::Exit(0) }
[Environment]::Exit(1)
