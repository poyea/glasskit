# Copyright (c) 2026 John Law
# SPDX-License-Identifier: MIT

# Checks that need a desktop: pictures of each popup kind and of a dialog, the idle CPU of
# a tray app, and the size of the example apps. CI runs it. It shows real windows, so do not
# run it on a desktop that you use.
#
#   pwsh -File tests/ui/ci.ps1 -Out shots

param(
    [string] $Out = 'ui-screenshots',
    # The largest release size of examples/quiz.exe, in bytes.
    [int] $SizeBudget = 400KB,
    # The most CPU time that the idle tray app can use in the idle window.
    [double] $IdleCpuSeconds = 0.25
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class GlassUi {
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr FindWindowExW(IntPtr parent, IntPtr after, string cls, IntPtr name);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
'@
# Window rectangles must be in physical pixels.
[GlassUi]::SetProcessDPIAware() | Out-Null

$root = Join-Path $PSScriptRoot '..\..'
$bin = Join-Path $root 'target\release\examples'
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$failures = @()

function Find-Window($Process, [string] $Class, [int] $TimeoutMs = 10000) {
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    while ([DateTime]::UtcNow -lt $deadline) {
        $hwnd = [IntPtr]::Zero
        while ($true) {
            $hwnd = [GlassUi]::FindWindowExW([IntPtr]::Zero, $hwnd, $Class, [IntPtr]::Zero)
            if ($hwnd -eq [IntPtr]::Zero) { break }
            $owner = 0
            [GlassUi]::GetWindowThreadProcessId($hwnd, [ref] $owner) | Out-Null
            if ($owner -eq $Process.Id) { return $hwnd }
        }
        if ($Process.HasExited) { throw "$($Process.ProcessName) ended before it showed $Class." }
        Start-Sleep -Milliseconds 100
    }
    throw "No $Class window after $TimeoutMs ms."
}

# A popup draws through the compositor, so only a copy of the screen shows it. A dialog is a
# normal window, and PrintWindow gets it also if another window covers it.
function Save-Picture([IntPtr] $Hwnd, [string] $Name, [switch] $FromScreen) {
    $r = New-Object GlassUi+RECT
    [GlassUi]::GetWindowRect($Hwnd, [ref] $r) | Out-Null
    $width, $height = ($r.Right - $r.Left), ($r.Bottom - $r.Top)
    $picture = New-Object System.Drawing.Bitmap $width, $height
    $graphics = [System.Drawing.Graphics]::FromImage($picture)
    try {
        if ($FromScreen) {
            $graphics.CopyFromScreen($r.Left, $r.Top, 0, 0, (New-Object System.Drawing.Size $width, $height))
        } else {
            $hdc = $graphics.GetHdc()
            [GlassUi]::PrintWindow($Hwnd, $hdc, 2) | Out-Null
            $graphics.ReleaseHdc($hdc)
        }
        $picture.Save((Join-Path $Out "$Name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $graphics.Dispose()
        $picture.Dispose()
    }
}

function Stop-App($Process) {
    if (-not $Process.HasExited) { $Process.Kill() }
    $Process.WaitForExit(5000) | Out-Null
}

function Step([string] $Name, [scriptblock] $Body) {
    try {
        & $Body
        Write-Host "ok   $Name" -ForegroundColor Green
    } catch {
        $script:failures += $Name
        Write-Host "FAIL $Name : $($_.Exception.Message)" -ForegroundColor Red
        if ($env:GITHUB_ACTIONS -eq 'true') { Write-Host "::error title=$Name::$($_.Exception.Message)" }
    }
}

$gallery = Join-Path $bin 'gallery.exe'
$kinds = ((& $gallery) -replace '^kinds:\s*', '') -split ',\s*' | Where-Object { $_ }
foreach ($kind in $kinds) {
    Step "popup $kind" {
        $p = Start-Process $gallery -ArgumentList $kind -PassThru
        try {
            $hwnd = Find-Window $p 'GlasskitChoicePopup'
            # Wait for the fade-in to end.
            Start-Sleep -Milliseconds 1000
            Save-Picture $hwnd "popup-$kind" -FromScreen
        } finally {
            Stop-App $p
        }
    }
}

Step 'dialog' {
    $p = Start-Process (Join-Path $bin 'dialog.exe') -PassThru
    try {
        $hwnd = Find-Window $p 'GlasskitDialog'
        Start-Sleep -Milliseconds 500
        Save-Picture $hwnd 'dialog'
        # WM_CLOSE must end the app, as the close button does.
        [GlassUi]::PostMessageW($hwnd, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
        if (-not $p.WaitForExit(5000)) { throw 'The dialog did not close on WM_CLOSE.' }
    } finally {
        Stop-App $p
    }
}

Step 'idle CPU' {
    # The quiz asks after 60 seconds, so the window below has no popup.
    $p = Start-Process (Join-Path $bin 'quiz.exe') -PassThru
    try {
        Find-Window $p 'GlasskitTray' | Out-Null
        Start-Sleep -Seconds 5
        $p.Refresh()
        $before = $p.TotalProcessorTime.TotalSeconds
        Start-Sleep -Seconds 40
        $p.Refresh()
        $used = $p.TotalProcessorTime.TotalSeconds - $before
        Write-Host ("idle CPU: {0:N3} s in 40 s" -f $used)
        if ($used -gt $IdleCpuSeconds) { throw ("The idle tray app used {0:N3} s of CPU." -f $used) }
    } finally {
        Stop-App $p
    }
}

Step 'size' {
    $rows = Get-ChildItem (Join-Path $bin '*.exe') | Sort-Object Name | ForEach-Object {
        "| $($_.Name) | $([math]::Round($_.Length / 1KB)) KB |"
    }
    $table = @('| Example | Release size |', '| --- | --- |') + $rows
    $table | Write-Host
    if ($env:GITHUB_STEP_SUMMARY) { $table | Add-Content $env:GITHUB_STEP_SUMMARY }
    $quiz = (Get-Item (Join-Path $bin 'quiz.exe')).Length
    if ($quiz -gt $SizeBudget) { throw "quiz.exe is $quiz bytes, over the budget of $SizeBudget." }
}

if ($failures.Count -gt 0) {
    Write-Host "$($failures.Count) check(s) failed." -ForegroundColor Red
    exit 1
}
Write-Host 'All desktop checks passed.' -ForegroundColor Green
