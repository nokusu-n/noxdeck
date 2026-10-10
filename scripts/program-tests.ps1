param([string]$App = 'target\e2e\debug\noxdeck.exe', [string]$Only = '')
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path -LiteralPath '.').Path
$exe = (Resolve-Path -LiteralPath $App).Path
$fake = (Resolve-Path -LiteralPath 'target\e2e\debug\test-launcher.exe').Path
$temporaryRoot = Join-Path $repo 'e2e\.tmp'
New-Item -ItemType Directory -Path $temporaryRoot -Force | Out-Null
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class NoxDeckWindows {
    private delegate bool Callback(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] private static extern bool EnumWindows(Callback cb, IntPtr data);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowTextW(IntPtr window, StringBuilder text, int length);
    [DllImport("user32.dll")] private static extern bool PostMessageW(IntPtr window, uint message, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] private static extern IntPtr SendMessageW(IntPtr window, uint message, IntPtr w, IntPtr l);
    public static string Title(IntPtr window) { var text = new StringBuilder(1024); GetWindowTextW(window, text, 1024); return text.ToString(); }
    public static IntPtr[] Visible(int pid) {
        var result = new List<IntPtr>();
        EnumWindows((w, d) => { uint p; GetWindowThreadProcessId(w, out p); if (p == pid && IsWindowVisible(w) && Title(w).StartsWith("Noxdeck")) result.Add(w); return true; }, IntPtr.Zero);
        return result.ToArray();
    }
    public static void Close(IntPtr window) { if (!PostMessageW(window, 0x0010, IntPtr.Zero, IntPtr.Zero)) throw new Exception("WM_CLOSE failed"); }
    public static void ActivateOwnStarter(IntPtr window) {
        uint owner; GetWindowThreadProcessId(window, out owner);
        if (owner != System.Diagnostics.Process.GetCurrentProcess().Id) throw new Exception("starter window has a foreign owner");
        // The window activation command goes only to this process's helper.
        // No global hotkey registration, input injection or cursor movement.
        SendMessageW(window, 0x0112, new IntPtr(0xF150), window);
    }
}
'@

function Assert([bool]$Condition, [string]$Reason) { if (-not $Condition) { throw $Reason } }
function Wait-Until([scriptblock]$Condition, [string]$Reason, [int]$Milliseconds = 5000) {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    while (-not (& $Condition)) {
        if ($watch.ElapsedMilliseconds -gt $Milliseconds) { throw $Reason }
        Start-Sleep -Milliseconds 20
    }
}
function Write-Json([string]$Path, $Value) { [IO.File]::WriteAllText($Path, ($Value | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false)) }

$script:program = $null
$script:runRoot = $null
$script:seen = [Collections.Generic.Dictionary[int,long]]::new()
function Remember-Process($Process) {
    try { $script:seen[[int]$Process.Id] = $Process.StartTime.Ticks } catch { }
}
function Owned-Process([int]$ProcessId) {
    $p = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    if ($null -eq $p) { return $null }
    try { if ($p.StartTime.Ticks -eq $script:seen[$ProcessId]) { return $p } } catch { }
    return $null
}
function Remember-Tree {
    if ($null -eq $script:program) { return }
    $all = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId)
    do {
        $added = $false
        foreach ($row in $all) {
            if (-not $script:seen.ContainsKey([int]$row.ProcessId) -and $script:seen.ContainsKey([int]$row.ParentProcessId) -and $null -ne (Owned-Process ([int]$row.ParentProcessId))) {
                $p = Get-Process -Id $row.ProcessId -ErrorAction SilentlyContinue
                if ($null -ne $p) { Remember-Process $p; $added = $true }
            }
        }
    } while ($added)
}
function Start-App([bool]$Tray = $false, [string]$After = 'stay', [string]$Action = '', [string]$Mode = 'ready') {
    $script:runRoot = Join-Path $temporaryRoot ('program-' + [guid]::NewGuid().ToString('N'))
    foreach ($dir in @('install','roaming','local','data','game')) { New-Item -ItemType Directory -Path (Join-Path $script:runRoot $dir) -Force | Out-Null }
    $install = Join-Path $script:runRoot 'install'
    $config = Get-Content -LiteralPath 'src-tauri\tests\fixtures\Battle.net.config' -Raw
    $config = $config.Replace('C:\\Fake\\Battle.net', $install.Replace('\','\\'))
    [IO.File]::WriteAllText((Join-Path $script:runRoot 'roaming\Battle.net.config'), $config, [Text.UTF8Encoding]::new($false))
    Copy-Item -LiteralPath $fake -Destination (Join-Path $install 'Battle.net Launcher.exe')
    Copy-Item -LiteralPath $fake -Destination (Join-Path $install 'Battle.net.exe')
    Copy-Item -LiteralPath $fake -Destination (Join-Path $script:runRoot 'game\NoxDeckProgramGame.exe')
    Write-Json (Join-Path $install 'scenario.json') @{accounts=@(@{email='userA@example.com'; id='aaaa0001'; tag='Alpha#1111'; mode=$Mode}); readiness='playable'; game_launch='stays'}
    Write-Json (Join-Path $script:runRoot 'data\accounts.json') @{schema=1; accounts=@(@{id='aaaa0001'; email='userA@example.com'; battletag='Alpha#1111'; login_state='ready'; last_login_at=$null})}
    Write-Json (Join-Path $script:runRoot 'data\settings.json') @{schema=1; sort='last_login'; dismissed=@{remember_login=$true; logout_warning=$true}; after_start=$After}
    $env:NOXDECK_BNET_ROAMING = Join-Path $script:runRoot 'roaming'
    $env:NOXDECK_BNET_LOCAL = Join-Path $script:runRoot 'local'
    $env:NOXDECK_DATA_DIR = Join-Path $script:runRoot 'data'
    $env:NOXDECK_GAME_EXE = 'NoxDeckProgramGame.exe'
    $env:NOXDECK_INSTANCE = 'program-' + [guid]::NewGuid().ToString('N')
    $env:NOXDECK_AUTOSTART_NAME = 'NoxDeck-Test-program-' + [guid]::NewGuid().ToString('N')
    $env:NOXDECK_LOCALE = 'en-US'
    if (-not $env:NOXDECK_OVERFAST_URL) { throw 'Run program tests through npm run test:program so the local rating stub is present' }
    $env:NOXDECK_PROGRAM_TEST = '1'
    $env:NOXDECK_TEST_START = $Action
    $env:NOXDECK_POLL_MS = '100'
    $env:NOXDECK_STAND_MS = '350'
    $env:NOXDECK_START_TIMEOUT_MS = '3000'
    $env:NOXDECK_GAME_TIMEOUT_MS = '1500'
    $env:NOXDECK_GAME_SETTLE_MS = '400'
    $env:NOXDECK_GAME_HANDOVER_MS = '1500'
    $env:NOXDECK_HANDOVER_PAUSE_MS = '400'
    $env:TEMP = $script:runRoot
    $env:TMP = $script:runRoot
    $arguments = @{FilePath=$exe; WorkingDirectory=$repo; WindowStyle='Hidden'; PassThru=$true}
    if ($Tray) { $arguments.ArgumentList = @('--tray') }
    $script:program = Start-Process @arguments
    $script:seen = [Collections.Generic.Dictionary[int,long]]::new()
    Remember-Process $script:program
}
function Alive { $script:program.Refresh(); return -not $script:program.HasExited }
function Windows { return [NoxDeckWindows]::Visible($script:program.Id) }
function Ready { return @((Windows) | Where-Object { [NoxDeckWindows]::Title($_) -eq 'Noxdeck [list ready]' }).Count -gt 0 }
function Close-Window {
    Wait-Until { @(Windows).Count -gt 0 } 'window did not appear'
    Remember-Tree
    [NoxDeckWindows]::Close(@(Windows)[0])
    Wait-Until { @(Windows).Count -eq 0 } 'cross did not remove the visible window'
    Start-Sleep -Milliseconds 200
    Assert (Alive) 'cross ended the program'
}
function Second-Start([bool]$Tray = $false) {
    $arguments = @{FilePath=$exe; WorkingDirectory=$repo; WindowStyle='Hidden'; PassThru=$true}
    if ($Tray) { $arguments.ArgumentList = @('--tray') }
    $second = Start-Process @arguments
    Remember-Process $second
    Assert ($second.WaitForExit(5000)) 'the second program did not exit'
    Assert ($second.ExitCode -eq 0) ('second start failed: ' + $second.ExitCode)
    Assert (Alive) 'the first program ended on a second start'
}
function Cleanup {
    Remember-Tree
    foreach ($processId in $script:seen.Keys) {
        $p = Owned-Process $processId
        if ($null -ne $p) { $p | Stop-Process -Force -ErrorAction SilentlyContinue }
    }
    # A successful app exit can leave the stand-in launcher/game reparented.
    if ($null -ne $script:runRoot) {
        Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($script:runRoot + '\', 'OrdinalIgnoreCase') } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
        $key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
        Assert ($env:NOXDECK_AUTOSTART_NAME.StartsWith('NoxDeck-Test-')) 'startup cleanup name is not isolated'
        $startupLeft = $null -ne (Get-ItemProperty -LiteralPath $key -Name $env:NOXDECK_AUTOSTART_NAME -ErrorAction SilentlyContinue)
        if ($startupLeft) { Remove-ItemProperty -LiteralPath $key -Name $env:NOXDECK_AUTOSTART_NAME }
        $target = (Resolve-Path -LiteralPath $script:runRoot).Path
        Assert ($target.StartsWith($temporaryRoot + '\', 'OrdinalIgnoreCase')) 'cleanup path escaped the workspace'
        for ($retry = 0; $retry -lt 10; $retry++) {
            try { Remove-Item -LiteralPath $target -Recurse -Force; break } catch { Start-Sleep -Milliseconds 200 }
        }
        Assert (-not (Test-Path -LiteralPath $target)) 'the test directory was left behind'
        Assert (-not $startupLeft) 'a test startup value needed cleanup'
    }
    $script:program = $null; $script:runRoot = $null
}

$tests = [ordered]@{
    'After a successful launcher start tray removes the window and keeps the core' = {
        Start-App -After 'tray' -Action 'start:aaaa0001'
        Wait-Until { Ready } 'initial list did not appear'
        Wait-Until { @(Windows).Count -eq 0 } 'successful start did not go to the tray' 7000
        Assert (Alive) 'tray setting ended the program'
    }
    'After a successful game start close ends the core' = {
        Start-App -After 'close' -Action 'game:aaaa0001'
        Wait-Until { Ready } 'initial list did not appear'
        Wait-Until { -not (Alive) } 'successful game start did not end the program' 8000
        Assert ($script:program.ExitCode -eq 0) 'close setting failed'
    }
    'A failed start from the tray shows the window even with close selected' = {
        Start-App -Tray $true -After 'close' -Action 'start:aaaa0001' -Mode 'login_screen'
        Wait-Until { Ready } 'a failed start did not show the list' 8000
        Assert (Alive) 'a failed start ended the program'
    }
    'Cross keeps the core and removes the window' = {
        Start-App; Close-Window
    }
    'Second manual start restores and focuses the first window' = {
        Start-App; Close-Window
        # A manual start comes from an active desktop process. The background
        # test runner otherwise has no Windows foreground permission to hand on.
        Add-Type -AssemblyName System.Windows.Forms
        $starter = New-Object System.Windows.Forms.Form
        $starter.Text = 'Noxdeck test: manual start'
        $starter.Width = 320; $starter.Height = 150
        $starter.StartPosition = [System.Windows.Forms.FormStartPosition]::CenterScreen
        try {
            $starter.TopMost = $true; $starter.Show(); $starter.Activate()
            [System.Windows.Forms.Application]::DoEvents()
            # Test setup switches only its own helper; the app must focus itself.
            Wait-Until {
                [System.Windows.Forms.Application]::DoEvents()
                [NoxDeckWindows]::ActivateOwnStarter($starter.Handle)
                [System.Windows.Forms.Application]::DoEvents()
                [NoxDeckWindows]::GetForegroundWindow() -eq $starter.Handle
            } 'the manual-start stand-in could not become active'
            $starter.TopMost = $false
            Second-Start
            Wait-Until { Ready } 'the existing instance did not show its list'
            Wait-Until { [NoxDeckWindows]::GetForegroundWindow() -eq @(Windows)[0] } 'the existing window did not come to the front'
        } finally { $starter.Dispose() }
    }
    '--tray has no window; a second --tray keeps it hidden; a manual start opens it' = {
        Start-App -Tray $true; Start-Sleep -Milliseconds 900
        Assert (Alive) '--tray exited'; Assert (@(Windows).Count -eq 0) '--tray showed a window'
        Second-Start -Tray $true; Start-Sleep -Milliseconds 200
        Assert (@(Windows).Count -eq 0) 'a second --tray showed a window'
        Second-Start; Wait-Until { Ready } 'manual start did not open the tray instance'
    }
    'Memory after 30 seconds and time from a manual show request to the list' = {
        Start-App; Wait-Until { Ready } 'the initial list was not rendered'; Close-Window
        Start-Sleep -Seconds 30; Remember-Tree
        $rows = @()
        foreach ($processId in $script:seen.Keys) {
            $p = Owned-Process $processId
            if ($null -ne $p) { $rows += [pscustomobject]@{name=$p.ProcessName; working_set_bytes=$p.WorkingSet64} }
        }
        $bytes = ($rows | Measure-Object -Property working_set_bytes -Sum).Sum
        $watch = [Diagnostics.Stopwatch]::StartNew(); Second-Start
        Wait-Until { Ready } 'the rebuilt list was not rendered'
        $ms = $watch.Elapsed.TotalMilliseconds
        Write-Output ('MEASURE memory_bytes=' + $bytes + ' processes=' + $rows.Count + ' time_to_list_ms=' + [Math]::Round($ms,3))
        Assert ($bytes -le 60000000) 'FR-019: tray memory exceeded 60 MB'
        Assert ($ms -le 1000) 'FR-020: the list took more than one second'
    }
}
$failed = 0
$focusCheck = 'Second manual start restores and focuses the first window'
$orderedTests = @($tests.GetEnumerator() | Where-Object { $_.Key -ne $focusCheck }) + @($tests.GetEnumerator() | Where-Object { $_.Key -eq $focusCheck })
foreach ($test in $orderedTests) {
    if ($Only -and $test.Key -notmatch $Only) { continue }
    try { & $test.Value; Write-Output ('PASS ' + $test.Key) }
    catch { $failed++; Write-Output ('FAIL ' + $test.Key + ': ' + $_.Exception.Message) }
    finally { Cleanup }
}
if ($failed -gt 0) { throw "$failed program test(s) failed" }
Write-Output 'All selected program tests passed; test processes, directories and startup entries are gone.'
