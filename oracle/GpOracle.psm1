Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

if (-not ('GpOracle.W32' -as [type])) {
  Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
namespace GpOracle {
  public static class W32 {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
    [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr h, EnumProc p, IntPtr l);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, string l);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr h, uint m, IntPtr w, string l, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr h, int id);
    [DllImport("user32.dll")] public static extern int GetDlgCtrlID(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint cmd);
    public static bool Focus(IntPtr h) {
      if (IsIconic(h)) ShowWindow(h, 9);
      if (SetForegroundWindow(h) && GetForegroundWindow() == h) return true;
      keybd_event(0x12, 0, 0, UIntPtr.Zero);
      keybd_event(0x12, 0, 2, UIntPtr.Zero);
      return SetForegroundWindow(h);
    }
  }
}
"@
}

$script:MenuRunspaces = New-Object System.Collections.ArrayList

function Get-GpProcess {
  Get-Process -Name GuitarPro -ErrorAction SilentlyContinue | Select-Object -First 1
}

function Get-GpVersion($Process) {
  (Get-Item -LiteralPath $Process.Path).VersionInfo.FileVersion
}

function Get-WindowText([IntPtr]$Handle) {
  $t = New-Object System.Text.StringBuilder 1024
  [void][GpOracle.W32]::GetWindowText($Handle, $t, 1024)
  $t.ToString()
}

function Get-ProcWindows([int]$ProcId) {
  $list = New-Object System.Collections.ArrayList
  $cb = [GpOracle.W32+EnumProc]{ param($h, $l)
    $p = [uint32]0
    [void][GpOracle.W32]::GetWindowThreadProcessId($h, [ref]$p)
    if ($p -eq $ProcId -and [GpOracle.W32]::IsWindowVisible($h)) {
      $c = New-Object System.Text.StringBuilder 256; [void][GpOracle.W32]::GetClassName($h, $c, 256)
      [void]$list.Add([pscustomobject]@{ Handle = $h; Title = (Get-WindowText $h); Class = $c.ToString(); Owner = [GpOracle.W32]::GetWindow($h, 4) })
    }
    $true }
  [void][GpOracle.W32]::EnumWindows($cb, [IntPtr]::Zero)
  $list
}

function Get-ChildWindows([IntPtr]$Parent) {
  $list = New-Object System.Collections.ArrayList
  $cb = [GpOracle.W32+EnumProc]{ param($h, $l)
    $c = New-Object System.Text.StringBuilder 256; [void][GpOracle.W32]::GetClassName($h, $c, 256)
    [void]$list.Add([pscustomobject]@{ Handle = $h; Id = [GpOracle.W32]::GetDlgCtrlID($h); Title = (Get-WindowText $h); Class = $c.ToString(); Visible = [GpOracle.W32]::IsWindowVisible($h) })
    $true }
  [void][GpOracle.W32]::EnumChildWindows($Parent, $cb, [IntPtr]::Zero)
  $list
}

function Wait-For([scriptblock]$Probe, [int]$TimeoutMs = 10000, [int]$StepMs = 100) {
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  while ($sw.ElapsedMilliseconds -lt $TimeoutMs) {
    $r = & $Probe
    if ($r) { return $r }
    Start-Sleep -Milliseconds $StepMs
  }
  $null
}

function Get-GpMain {
  $p = Get-GpProcess
  if (-not $p) { return $null }
  $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty, $p.Id)
  $all = [System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)
  foreach ($w in $all) { if ($w.Current.AutomationId -eq 'MainWindow') { return $w } }
  $null
}

$script:MainHandle = [IntPtr]::Zero

function Get-GpMainHandle {
  if ($script:MainHandle -ne [IntPtr]::Zero -and [GpOracle.W32]::IsWindow($script:MainHandle)) { return $script:MainHandle }
  $p = Get-GpProcess
  if (-not $p) { return [IntPtr]::Zero }
  $candidates = @(Get-ProcWindows $p.Id | Where-Object { $_.Class -like 'Qt*QWindowIcon' -and $_.Title.EndsWith(' - Guitar Pro 8') -and [GpOracle.W32]::GetWindow($_.Handle, 4) -eq [IntPtr]::Zero })
  if ($candidates.Count -eq 1) {
    $script:MainHandle = $candidates[0].Handle
    return $script:MainHandle
  }
  $m = Get-GpMain
  if (-not $m) { return [IntPtr]::Zero }
  $script:MainHandle = [IntPtr]$m.Current.NativeWindowHandle
  $script:MainHandle
}

function Get-GpTitle { Get-WindowText (Get-GpMainHandle) }

function Assert-MainKnown([IntPtr]$Main) {
  if ($Main -eq [IntPtr]::Zero) { throw 'Guitar Pro main window is unknown; refusing to dismiss or close any window' }
}

function Test-CloseAllowed([IntPtr]$Handle, [IntPtr]$Owner, [IntPtr]$Main) {
  $Main -ne [IntPtr]::Zero -and $Handle -ne [IntPtr]::Zero -and $Handle -ne $Main -and $Owner -eq $Main
}

function Select-GpModals([object[]]$Windows, [IntPtr]$Main) {
  Assert-MainKnown $Main
  @($Windows | Where-Object {
      $_.Class -ne 'SysShadow' -and $_.Class -notlike '*ToolTip*' -and $_.Class -notlike '*Popup*' -and (Test-CloseAllowed $_.Handle $_.Owner $Main)
    })
}

function Get-GpModals {
  $p = Get-GpProcess
  if (-not $p) { return @() }
  Select-GpModals @(Get-ProcWindows $p.Id) (Get-GpMainHandle)
}

function Close-OwnedWindow([IntPtr]$Handle, [IntPtr]$Main) {
  Assert-MainKnown $Main
  if (-not (Test-CloseAllowed $Handle ([GpOracle.W32]::GetWindow($Handle, 4)) $Main)) { return $false }
  [void][GpOracle.W32]::PostMessage($Handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
  $true
}

function Get-GpPopups {
  $p = Get-GpProcess
  Get-ProcWindows $p.Id | Where-Object { $_.Class -like '*Popup*' } | ForEach-Object { [System.Windows.Automation.AutomationElement]::FromHandle($_.Handle) }
}

function ConvertTo-LabelKey([string]$Text) {
  if ($null -eq $Text) { return '' }
  $t = $Text.Replace('&', '').Replace([string][char]0x2026, '').Trim()
  while ($t.EndsWith('.')) { $t = $t.Substring(0, $t.Length - 1) }
  $t.Trim().ToLowerInvariant()
}

function Find-MenuItem($Root, [string[]]$Alternatives) {
  $type = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::MenuItem)
  $items = $Root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $type)
  foreach ($alt in $Alternatives) {
    if ($alt.StartsWith('aid:')) {
      $aid = $alt.Substring(4)
      foreach ($x in $items) { if ($x.Current.AutomationId -eq $aid) { return $x } }
    }
  }
  foreach ($alt in $Alternatives) {
    if ($alt.StartsWith('aid:')) { continue }
    $key = ConvertTo-LabelKey $alt
    foreach ($x in $items) { if ((ConvertTo-LabelKey $x.Current.Name) -eq $key) { return $x } }
  }
  $null
}

function Invoke-El($El) {
  $p = $null
  if ($El.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$p)) { $p.Invoke(); return $true }
  $false
}

function Expand-El($El) {
  $p = $null
  if ($El.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$p)) { $p.Expand(); return $true }
  $false
}

function Open-MenuPath([string[]]$Path) {
  $main = Get-GpMain
  if (-not $main) { throw 'Guitar Pro main window not found' }
  $first = $Path[0].Split('|')
  $item = Find-MenuItem $main $first
  if (-not $item) { throw "menu not found: $($first -join '|')" }
  if ($Path.Count -eq 1) { return $item }
  [void](Expand-El $item)
  for ($i = 1; $i -lt $Path.Count; $i++) {
    $alts = $Path[$i].Split('|')
    $found = Wait-For { foreach ($pp in Get-GpPopups) { $x = Find-MenuItem $pp $alts; if ($x) { return $x } } } 3000
    if (-not $found) { throw "menu item not found: $($alts -join '|')" }
    if ($i -eq $Path.Count - 1) { return $found }
    [void](Expand-El $found)
  }
}

function Invoke-MenuAsync([string[]]$Path) {
  $leaf = Open-MenuPath $Path
  $ps = [PowerShell]::Create()
  [void]$ps.AddScript({ param($el) $p = $null; if ($el.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$p)) { $p.Invoke() } }).AddArgument($leaf)
  $h = $ps.BeginInvoke()
  [void]$script:MenuRunspaces.Add([pscustomobject]@{ PS = $ps; Handle = $h })
  $h
}

function Close-MenuRunspaces {
  $pending = New-Object System.Collections.ArrayList
  foreach ($r in $script:MenuRunspaces) {
    if ($r.Handle.IsCompleted) { try { $r.PS.Dispose() } catch { } } else { [void]$pending.Add($r) }
  }
  $script:MenuRunspaces = $pending
  $pending.Count
}

function Send-KeysTo([IntPtr]$Handle, [string]$Keys) {
  [void][GpOracle.W32]::Focus($Handle)
  $ok = Wait-For { if ([GpOracle.W32]::GetForegroundWindow() -eq $Handle) { $true } } 1500 50
  if (-not $ok) { return $false }
  [System.Windows.Forms.SendKeys]::SendWait($Keys)
  $true
}

function Close-GpMenus {
  $main = Get-GpMainHandle
  Assert-MainKnown $main
  for ($i = 0; $i -lt 4; $i++) {
    if (@(Get-GpPopups).Count -eq 0) { return $true }
    if (-not (Send-KeysTo $main '{ESC}')) { return $false }
    Start-Sleep -Milliseconds 150
  }
  @(Get-GpPopups).Count -eq 0
}

function Clear-GpModals([int]$MaxAttempts = 3) {
  $main = Get-GpMainHandle
  Assert-MainKnown $main
  $seen = New-Object System.Collections.ArrayList
  for ($i = 0; $i -lt $MaxAttempts; $i++) {
    if (@(Get-GpPopups).Count -gt 0) { [void](Close-GpMenus) }
    $mods = Get-GpModals
    if ($mods.Count -eq 0) { return [pscustomobject]@{ Cleared = $true; Seen = $seen } }
    foreach ($m in $mods) {
      $label = "$($m.Title) [$($m.Class)]"
      if (-not $seen.Contains($label)) { [void]$seen.Add($label) }
      if (-not [GpOracle.W32]::IsWindow($m.Handle)) { continue }
      if (-not (Test-CloseAllowed $m.Handle ([GpOracle.W32]::GetWindow($m.Handle, 4)) $main)) { continue }
      if ($i -eq 0) {
        [void](Send-KeysTo $m.Handle '{ESC}')
      } else {
        [void](Close-OwnedWindow $m.Handle $main)
      }
    }
    [void](Wait-For { if ((Get-GpModals).Count -eq 0) { $true } } 1500 100)
  }
  [pscustomobject]@{ Cleared = ((Get-GpModals).Count -eq 0); Seen = $seen }
}

function Wait-NewWindow([object[]]$Before, [int]$TimeoutMs) {
  $known = @{}
  foreach ($w in $Before) { $known[[string]$w.Handle] = $true }
  Wait-For {
    $p = Get-GpProcess
    if (-not $p) { return $null }
    $main = Get-GpMainHandle
    Get-ProcWindows $p.Id | Where-Object {
      -not $known.ContainsKey([string]$_.Handle) -and $_.Handle -ne $main -and $_.Class -ne 'SysShadow' -and $_.Class -notlike '*Popup*' -and $_.Class -notlike '*ToolTip*'
    } | Select-Object -First 1
  } $TimeoutMs
}

function Complete-FileDialog([IntPtr]$Dialog, [string]$Path) {
  $edit = Wait-For { Get-ChildWindows $Dialog | Where-Object { $_.Class -eq 'Edit' -and $_.Visible -and ($_.Id -eq 1001 -or $_.Id -eq 1148) } | Select-Object -First 1 } 3000
  if (-not $edit) { throw 'filename edit not found' }
  $r = [IntPtr]::Zero
  [void][GpOracle.W32]::SendMessageTimeout($edit.Handle, 0x000C, [IntPtr]::Zero, $Path, 2, 3000, [ref]$r)
  $ok = [GpOracle.W32]::GetDlgItem($Dialog, 1)
  [void][GpOracle.W32]::PostMessage($Dialog, 0x0111, [IntPtr]1, $ok)
}

function Save-Screenshot([string]$Path) {
  try {
    $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size)
    $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    $Path
  } catch { $null }
}

function Format-El($El) {
  $c = $El.Current
  "{0} name='{1}' aid='{2}' class='{3}' {4}" -f $c.ControlType.ProgrammaticName, $c.Name, $c.AutomationId, $c.ClassName, $c.BoundingRectangle
}

function Write-Tree($El, [int]$Depth = 3, [int]$D = 0) {
  if ($D -gt $Depth) { return }
  Write-Output (('  ' * $D) + (Format-El $El))
  $w = [System.Windows.Automation.TreeWalker]::ControlViewWalker
  $ch = $w.GetFirstChild($El)
  while ($ch -ne $null) { Write-Tree $ch $Depth ($D + 1); $ch = $w.GetNextSibling($ch) }
}

function Get-ModalDump {
  $lines = New-Object System.Collections.ArrayList
  foreach ($m in Get-GpModals) {
    [void]$lines.Add("modal '$($m.Title)' $($m.Class)")
    try { foreach ($l in Write-Tree ([System.Windows.Automation.AutomationElement]::FromHandle($m.Handle)) 3 0) { [void]$lines.Add("  $l") } } catch { }
  }
  $lines
}

function Find-GpErrorToast {
  $main = Get-GpMain
  if (-not $main) { return $null }
  foreach ($name in @('Error', 'Erro')) {
    $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $name)
    foreach ($x in $main.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)) {
      if ($x.Current.ControlType -eq [System.Windows.Automation.ControlType]::Text) { return $x }
    }
  }
  $null
}

function Close-GpErrorToast {
  $err = Find-GpErrorToast
  if (-not $err) { return $null }
  $er = $err.Current.BoundingRectangle
  $main = Get-GpMain
  $texts = New-Object System.Collections.ArrayList
  $tc = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Text)
  foreach ($t in $main.FindAll([System.Windows.Automation.TreeScope]::Descendants, $tc)) {
    $r = $t.Current.BoundingRectangle
    if ([math]::Abs($r.Left - $er.Left) -lt 400 -and $r.Top -ge $er.Top -and $r.Top -lt ($er.Top + 120)) { [void]$texts.Add($t.Current.Name) }
  }
  $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ClassNameProperty, 'am::gui::PushButton')
  foreach ($b in $main.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)) {
    $r = $b.Current.BoundingRectangle
    if ($r.Left -gt $er.Right -and [math]::Abs($r.Top - $er.Top) -lt 6) { [void](Invoke-El $b); break }
  }
  ($texts -join ' / ')
}

function Close-ExplorerFolder([string]$Folder, [int]$WaitMs = 0) {
  $target = [System.IO.Path]::GetFullPath($Folder).TrimEnd('\')
  $shell = New-Object -ComObject Shell.Application
  $script:closedExplorer = 0
  $probe = {
    foreach ($w in @($shell.Windows())) {
      try {
        $url = [string]$w.LocationURL
        if ($url -and ([Uri]$url).LocalPath.TrimEnd('\') -ieq $target) { $w.Quit(); $script:closedExplorer++ }
      } catch { }
    }
    if ($script:closedExplorer -gt 0) { $true }
  }
  if ($WaitMs -gt 0) { [void](Wait-For $probe $WaitMs 250) } else { [void](& $probe) }
  $script:closedExplorer
}

function Write-Utf8File([string]$Path, [string]$Text) {
  [System.IO.File]::WriteAllText($Path, $Text, (New-Object System.Text.UTF8Encoding $false))
}

Export-ModuleMember -Function *
