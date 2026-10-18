param(
  [Parameter(Mandatory = $true)][string]$Source,
  [Parameter(Mandatory = $true)][string]$OutDir,
  [int]$DialogTimeoutMs = 8000,
  [int]$StepTimeoutMs = 60000,
  [int]$MaxAttempts = 3
)

$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'GpOracle.psm1') -Force -DisableNameChecking -WarningAction SilentlyContinue

$Menus = @{
  Open     = @('File', 'Open...')
  Midi     = @('File', 'Export', 'MIDI...')
  MusicXml = @('File', 'Export', 'MusicXML...')
  Pdf      = @('File', 'Export', 'PDF...')
  Gp5      = @('File', 'Export', 'GP5...')
  Gpx      = @('File', 'Export', 'GPX...')
  Png      = @('File', 'Export', 'PNG...')
  SaveAs   = @('File', 'Save As...|Save as')
  Close    = @('File', 'Close')
}
$GpSuffix = ' - Guitar Pro 8'

$Diag = Join-Path $OutDir 'diagnostics'
New-Item -ItemType Directory -Force -Path $Diag | Out-Null
Write-Utf8File (Join-Path $OutDir 'pid.txt') ([string]$PID)

$Steps = New-Object System.Collections.ArrayList
$ModalsAtStart = New-Object System.Collections.ArrayList
$Notes = New-Object System.Collections.ArrayList
$script:LastMenuError = $null
$Result = [ordered]@{
  ok              = $false
  gp_running      = $false
  gp_version      = $null
  gp_pid          = $null
  source          = $Source
  error           = $null
  modals_at_start = $ModalsAtStart
  notes           = $Notes
  steps           = $Steps
}
$script:Attempts = 1

function Write-Result {
  Write-Utf8File (Join-Path $OutDir 'result.json') ($Result | ConvertTo-Json -Depth 6)
}

function Invoke-Step([string]$Name, [scriptblock]$Body) {
  $rec = [ordered]@{ name = $Name; status = 'ok'; ms = 0; attempts = 1; error = $null; screenshot = $null }
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
  [void]$Steps.Add([pscustomobject]$rec)
  [void](Close-MenuRunspaces)
  $rec.status -eq 'ok'
}

function Skip-Step([string]$Name, [string]$Why) {
  [void]$Steps.Add([pscustomobject][ordered]@{ name = $Name; status = 'skipped'; ms = 0; attempts = 0; error = $Why; screenshot = $null })
}

function Open-GpDialog([string[]]$Menu, [string]$Kind) {
  for ($a = 1; $a -le $MaxAttempts; $a++) {
    $script:Attempts = $a
    $c = Clear-GpModals $MaxAttempts
    if (-not $c.Cleared) { throw "cannot clear dialogs before $($Menu -join ' > '): $($c.Seen -join ', ')" }
    $before = @(Get-ProcWindows (Get-GpProcess).Id)
    try {
      [void](Invoke-MenuAsync $Menu)
    } catch {
      $script:LastMenuError = $_.Exception.Message
      [void](Close-GpMenus)
      continue
    }
    $w = Wait-NewWindow $before $DialogTimeoutMs
    if ($w) {
      if ($Kind -eq 'native' -and $w.Class -ne '#32770') { throw "expected a file dialog for $($Menu -join ' > '), got '$($w.Title)' [$($w.Class)]" }
      if ($Kind -eq 'quick' -and $w.Class -eq '#32770') { throw "expected an export window for $($Menu -join ' > '), got file dialog '$($w.Title)'" }
      return $w
    }
    [void](Close-GpMenus)
  }
  throw "dialog for $($Menu -join ' > ') never appeared after $MaxAttempts attempts; last menu error: $script:LastMenuError"
}

function Test-FileReady([string]$Path) {
  try {
    $s = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::None)
    $s.Dispose()
    $true
  } catch { $false }
}

function Wait-StableFile([string]$Path) {
  $script:lastLen = -1
  Wait-For {
    if (Test-Path -LiteralPath $Path) {
      $l = (Get-Item -LiteralPath $Path).Length
      if ($l -gt 0 -and $l -eq $script:lastLen -and (Test-FileReady $Path)) { return $true }
      $script:lastLen = $l
    }
    $null
  } $StepTimeoutMs 300
}

function Get-ModalTitles { (@(Get-GpModals) | ForEach-Object { "'$($_.Title)' [$($_.Class)]" }) -join ', ' }

function Export-ViaFileDialog([string[]]$Menu, [string]$Target) {
  if (Test-Path -LiteralPath $Target) { Remove-Item -LiteralPath $Target -Force }
  $w = Open-GpDialog $Menu 'native'
  Complete-FileDialog $w.Handle $Target
  $gone = Wait-For { if (-not [GpOracle.W32]::IsWindow($w.Handle)) { $true } } 10000
  if (-not $gone) { throw "'$($w.Title)' did not close" }
  $script:lastLen = -1
  $done = Wait-For {
    $report = @(Get-GpModals | Where-Object { $_.Title -eq "Export result$GpSuffix" }) | Select-Object -First 1
    if ($report) {
      [void]$Notes.Add("$(Split-Path -Leaf $Target): dismissed '$($report.Title)'")
      [void][GpOracle.W32]::PostMessage($report.Handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
      [void](Wait-For { if (-not [GpOracle.W32]::IsWindow($report.Handle)) { $true } } 5000)
      return $null
    }
    if (Test-Path -LiteralPath $Target) {
      $l = (Get-Item -LiteralPath $Target).Length
      if ($l -gt 0 -and $l -eq $script:lastLen -and (Test-FileReady $Target)) { return $true }
      $script:lastLen = $l
    }
    $null
  } $StepTimeoutMs 300
  if (-not $done) { throw "no stable output at $Target; open dialogs: $(Get-ModalTitles)" }
}

function Get-FolderFiles([string]$Folder, [string]$Filter) {
  @(Get-ChildItem -LiteralPath $Folder -Filter $Filter -File | ForEach-Object { $_.FullName })
}

function Wait-NewFiles([string]$Folder, [string]$Filter, [string[]]$Before) {
  $script:lastSig = ''
  Wait-For {
    $now = @(Get-FolderFiles $Folder $Filter | Where-Object { $Before -notcontains $_ })
    if ($now.Count -eq 0) { return $null }
    $sig = ($now | ForEach-Object { "$_=$((Get-Item -LiteralPath $_).Length)" }) -join ';'
    $ready = @($now | Where-Object { -not (Test-FileReady $_) }).Count -eq 0
    if ($sig -eq $script:lastSig -and $ready) { return , $now }
    $script:lastSig = $sig
    $null
  } $StepTimeoutMs 500
}

function Export-ViaQuickDialog([string[]]$Menu, [string]$Folder, [string]$Filter) {
  $before = Get-FolderFiles $Folder $Filter
  $w = Open-GpDialog $Menu 'quick'
  Start-Sleep -Milliseconds 300
  if (-not (Send-KeysTo $w.Handle '{ENTER}')) { throw "'$($w.Title)' never reached the foreground; ENTER not sent" }
  $gone = Wait-For { if (-not [GpOracle.W32]::IsWindow($w.Handle)) { $true } } 10000
  if (-not $gone) { throw "'$($w.Title)' did not close after ENTER" }
  $files = Wait-NewFiles $Folder $Filter $before
  $closed = Close-ExplorerFolder $Folder 4000
  if ($closed -gt 0) { [void]$Notes.Add("closed $closed Explorer window(s) opened on the job folder") }
  if (-not $files) { throw "no new $Filter in $Folder; open dialogs: $(Get-ModalTitles)" }
  $files
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

$name = [System.IO.Path]::GetFileNameWithoutExtension($Source)
$docFolder = Split-Path -Parent $Source
$docTitle = "$name$GpSuffix"
$opened = $false
$currentTitle = $docTitle

try {
  $ready = Invoke-Step 'precheck' {
    if (-not (Test-Path -LiteralPath $Source)) { throw "source not found: $Source" }
    if ((Get-GpMainHandle) -eq [IntPtr]::Zero) { throw 'Guitar Pro main window not found' }
    $c = Clear-GpModals $MaxAttempts
    foreach ($s in $c.Seen) { [void]$ModalsAtStart.Add($s) }
    if (-not $c.Cleared) { throw "dialogs still open after $MaxAttempts attempts: $($c.Seen -join ', ')" }
    if (-not (Get-GpMain)) { throw 'Guitar Pro main window not reachable through UI Automation' }
    if ((Get-GpTitle) -eq $docTitle) { throw "a document named '$name' is already active in Guitar Pro; close it first" }
  }

  if ($ready) {
    $opened = Invoke-Step 'open' {
      $w = Open-GpDialog $Menus.Open 'native'
      Complete-FileDialog $w.Handle $Source
      $gone = Wait-For { if (-not [GpOracle.W32]::IsWindow($w.Handle)) { $true } } 10000
      if (-not $gone) { throw "'$($w.Title)' did not close" }
      $ok = Wait-For { if ((Get-GpTitle) -eq $docTitle) { $true } } $StepTimeoutMs 200
      if (-not $ok) {
        $toast = Close-GpErrorToast
        if ($toast) { throw "Guitar Pro reported: $toast" }
        throw "document did not open; title is '$(Get-GpTitle)'"
      }
    }
  } else {
    Skip-Step 'open' 'precheck failed'
  }

  if ($opened) {
    [void](Invoke-Step 'midi' { Export-ViaFileDialog $Menus.Midi (Join-Path $OutDir 'export.mid') })
    [void](Invoke-Step 'musicxml' {
        $xml = Join-Path $OutDir 'export.xml'
        $final = Join-Path $OutDir 'export.musicxml'
        Export-ViaFileDialog $Menus.MusicXml $xml
        Move-Item -LiteralPath $xml -Destination $final -Force
      })
    [void](Invoke-Step 'gp5' { Export-ViaFileDialog $Menus.Gp5 (Join-Path $OutDir 'export.gp5') })
    [void](Invoke-Step 'gpx' { Export-ViaFileDialog $Menus.Gpx (Join-Path $OutDir 'export.gpx') })
    [void](Invoke-Step 'pdf' {
        $files = @(Export-ViaQuickDialog $Menus.Pdf $docFolder '*.pdf')
        if ($files.Count -ne 1) { throw "expected one PDF, got $($files.Count)" }
        Move-Item -LiteralPath $files[0] -Destination (Join-Path $OutDir 'export.pdf') -Force
      })
    [void](Invoke-Step 'png' {
        $files = @(Export-ViaQuickDialog $Menus.Png $docFolder '*.png')
        $pngDir = Join-Path $OutDir 'png'
        New-Item -ItemType Directory -Force -Path $pngDir | Out-Null
        foreach ($f in $files) { Move-Item -LiteralPath $f -Destination (Join-Path $pngDir (Split-Path -Leaf $f)) -Force }
      })
    [void](Invoke-Step 'saveas' {
        $target = Join-Path $OutDir 'resave.gp'
        Export-ViaFileDialog $Menus.SaveAs $target
        $t = "resave$GpSuffix"
        if (-not (Wait-For { if ((Get-GpTitle) -eq $t) { $true } } 5000)) { throw "title did not switch to '$t'" }
        $script:currentTitle = $t
      })
  } else {
    foreach ($s in @('midi', 'musicxml', 'gp5', 'gpx', 'pdf', 'png', 'saveas')) { Skip-Step $s 'document not open' }
  }
} finally {
  if ($opened) {
    [void](Invoke-Step 'close' {
        $c = Clear-GpModals $MaxAttempts
        if (-not $c.Cleared) { throw "cannot clear dialogs before close: $($c.Seen -join ', ')" }
        $before = Get-GpTitle
        if ($before -ne $script:currentTitle -and $before -ne $docTitle) { throw "active document is '$before', not ours; left open" }
        [void](Invoke-MenuAsync $Menus.Close)
        $closed = Wait-For { $t = Get-GpTitle; if ($t -ne $before -and $t -ne '') { $true } } $StepTimeoutMs 200
        if (-not $closed) { throw "document did not close; open dialogs: $(Get-ModalTitles)" }
      })
  } else {
    Skip-Step 'close' 'document not open'
  }
  try { [void](Close-ExplorerFolder $docFolder 1500) } catch { }
  try { [void](Close-GpMenus) } catch { }
  $Result.pending_menu_invokes = Close-MenuRunspaces
  $Result.ok = (@($Steps | Where-Object { $_.status -ne 'ok' }).Count -eq 0)
  Write-Result
}

if ($Result.ok) { [Environment]::Exit(0) }
[Environment]::Exit(1)
