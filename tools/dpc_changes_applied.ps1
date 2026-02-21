# DPC Latency Optimizations - APPLIED CHANGES
# This script documents and verifies all changes applied to iem.lan
# Created: 2026-02-21
# Purpose: Fix DPC latency spikes from AMD GPU driver during window composition

Write-Output "=== DPC LATENCY OPTIMIZATIONS - CURRENT STATE ==="
Write-Output "Date applied: 2026-02-21"
Write-Output ""

# === ROUND 1 (Rebooted) ===
Write-Output "--- ROUND 1: MMCSS + Visual Effects ---"

$v = (Get-ItemProperty "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile").SystemResponsiveness
Write-Output "MMCSS SystemResponsiveness: $v (should be 0, default=20)"

$pa = Get-ItemProperty "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Pro Audio"
Write-Output "Pro Audio SFIO Priority: $($pa.'SFIO Priority') (should be High)"
Write-Output "Pro Audio Clock Rate: $($pa.'Clock Rate') (should be 10000)"
Write-Output "Pro Audio GPU Priority: $($pa.'GPU Priority') (should be 8)"
Write-Output "Pro Audio Priority: $($pa.Priority) (should be 6)"

$sid = "S-1-5-21-15251608-2992027975-817598959-1001"
$trans = (Get-ItemProperty "Registry::HKU\$sid\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize" -ErrorAction SilentlyContinue).EnableTransparency
Write-Output "Transparency: $trans (should be 0=disabled)"

$ma = (Get-ItemProperty "Registry::HKU\$sid\Control Panel\Desktop" -ErrorAction SilentlyContinue).MinAnimate
$dfw = (Get-ItemProperty "Registry::HKU\$sid\Control Panel\Desktop" -ErrorAction SilentlyContinue).DragFullWindows
$ta = (Get-ItemProperty "Registry::HKU\$sid\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced" -ErrorAction SilentlyContinue).TaskbarAnimations
Write-Output "MinAnimate: $ma (should be 0)"
Write-Output "DragFullWindows: $dfw (should be 0)"
Write-Output "TaskbarAnimations: $ta (should be 0)"

Write-Output ""

# === ROUND 2 (Rebooted) ===
Write-Output "--- ROUND 2: Interrupt Affinity + GPU Settings ---"

# Yamaha AIC128-D: VEN_1A39&DEV_0004 (PCI\VEN_1A39&DEV_0004&SUBSYS_E04E1A39&REV_01\6&291EA6AB&0&0020020A)
$yamahaPath = "HKLM:\SYSTEM\CurrentControlSet\Enum\PCI\VEN_1A39&DEV_0004&SUBSYS_E04E1A39&REV_01\6&291EA6AB&0&0020020A\Device Parameters\Interrupt Management\Affinity Policy"
if (Test-Path $yamahaPath) {
    $yp = Get-ItemProperty $yamahaPath -ErrorAction SilentlyContinue
    $yamCore = if ($yp.AssignmentSetOverride) { [BitConverter]::ToString($yp.AssignmentSetOverride) } else { "not set" }
    Write-Output "Yamaha DevicePolicy: $($yp.DevicePolicy) (should be 4=SpecifiedProcessors)"
    Write-Output "Yamaha AssignmentSetOverride: $yamCore (should be 00-C0=cores 14-15)"
    Write-Output "Yamaha DevicePriority: $($yp.DevicePriority) (should be 3=High)"
} else {
    Write-Output "WARNING: Yamaha Affinity Policy path NOT FOUND - interrupt affinity NOT applied!"
}

# AMD Radeon RX 5500 XT: VEN_1002&DEV_7340 (PCI\VEN_1002&DEV_7340&SUBSYS_38221462&REV_C5\6&18F95EC&0&00000019)
$gpuPath = "HKLM:\SYSTEM\CurrentControlSet\Enum\PCI\VEN_1002&DEV_7340&SUBSYS_38221462&REV_C5\6&18F95EC&0&00000019\Device Parameters\Interrupt Management\Affinity Policy"
if (Test-Path $gpuPath) {
    $gp = Get-ItemProperty $gpuPath -ErrorAction SilentlyContinue
    $gpuCore = if ($gp.AssignmentSetOverride) { [BitConverter]::ToString($gp.AssignmentSetOverride) } else { "not set" }
    Write-Output "GPU DevicePolicy: $($gp.DevicePolicy) (should be 4=SpecifiedProcessors)"
    Write-Output "GPU AssignmentSetOverride: $gpuCore (should be 0F-00=cores 0-3)"
    Write-Output "GPU DevicePriority: $($gp.DevicePriority) (should be 1=Low)"
} else {
    Write-Output "WARNING: GPU Affinity Policy path NOT FOUND - interrupt affinity NOT applied!"
}

$hwSch = (Get-ItemProperty "HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers").HwSchMode
Write-Output "HW GPU Scheduling: $hwSch (should be 1=disabled, default=2)"

# Check ULPS
$ulps = Get-ChildItem "HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}" -ErrorAction SilentlyContinue | ForEach-Object {
    (Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue).EnableULPS
} | Where-Object { $_ -ne $null } | Select-Object -First 1
Write-Output "AMD ULPS: $ulps (should be 0=disabled)"

# BCDEdit
$bcd = bcdedit /enum "{current}" 2>$null
$ddt = if ($bcd -match "disabledynamictick\s+Yes") { "Yes" } else { "not set" }
$upt = if ($bcd -match "useplatformtick\s+Yes") { "Yes" } else { "not set" }
$tsc = if ($bcd -match "tscsyncpolicy\s+Enhanced") { "Enhanced" } else { "not set" }
Write-Output "BCDEdit disabledynamictick: $ddt (should be Yes)"
Write-Output "BCDEdit useplatformtick: $upt (should be Yes)"
Write-Output "BCDEdit tscsyncpolicy: $tsc (should be Enhanced)"

# Deep sleep
$dsd = Get-ChildItem "HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}" -ErrorAction SilentlyContinue | ForEach-Object {
    (Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue).PP_SclkDeepSleepDisable
} | Where-Object { $_ -ne $null } | Select-Object -First 1
Write-Output "PP_SclkDeepSleepDisable: $dsd (should be 1)"

# MMCSS Affinity
$affinity = (Get-ItemProperty "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Pro Audio" -ErrorAction SilentlyContinue).Affinity
Write-Output "Pro Audio Affinity: $affinity (should be 49152=0xC000=cores 14-15)"

Write-Output ""

# === ROUND 3 (NOT YET REBOOTED) ===
Write-Output "--- ROUND 3: Aggressive GPU/DWM (requires reboot) ---"

$dpg = Get-ChildItem "HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}" -ErrorAction SilentlyContinue | ForEach-Object {
    (Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue).DisablePowerGating
} | Where-Object { $_ -ne $null } | Select-Object -First 1
Write-Output "DisablePowerGating: $dpg (should be 1)"

$dpm = Get-ChildItem "HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}" -ErrorAction SilentlyContinue | ForEach-Object {
    (Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue).DisableDPM
} | Where-Object { $_ -ne $null } | Select-Object -First 1
Write-Output "DisableDPM: $dpm (should be 1)"

$kcp = Get-ChildItem "HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}" -ErrorAction SilentlyContinue | ForEach-Object {
    (Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue).KMD_EnableComputePreemption
} | Where-Object { $_ -ne $null } | Select-Object -First 1
Write-Output "KMD_EnableComputePreemption: $kcp (should be 0)"

$dfex = (Get-ItemProperty "HKLM:\SOFTWARE\Microsoft\Windows\DWM" -ErrorAction SilentlyContinue).DisableFlipExModel
$otm = (Get-ItemProperty "HKLM:\SOFTWARE\Microsoft\Windows\DWM" -ErrorAction SilentlyContinue).OverlayTestMode
Write-Output "DWM DisableFlipExModel: $dfex (should be 1)"
Write-Output "DWM OverlayTestMode: $otm (should be 5)"

$gdvr = (Get-ItemProperty "HKLM:\SOFTWARE\Policies\Microsoft\Windows\GameDVR" -ErrorAction SilentlyContinue).AllowGameDVR
Write-Output "GameDVR AllowGameDVR: $gdvr (should be 0)"

Write-Output ""
Write-Output "=== DISABLED DEVICES (via Device Manager) ==="
Write-Output "- AMD High Definition Audio Device (disabled)"
Write-Output "- AMD Streaming Audio Device (disabled)"
Write-Output "- USB Virtual Display (disabled, if present)"
Write-Output ""
Write-Output "=== FILES ON SYSTEM ==="
Write-Output "C:\Users\Public\dpc_changes_applied.ps1 - This status check script"
Write-Output "C:\Users\Public\dpc_restore.ps1 - Restores ALL changes to defaults"
