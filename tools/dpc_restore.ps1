# DPC Latency Optimization - RESTORE DEFAULTS Script
# Created: 2026-02-21
# Run this script as Administrator to undo all DPC optimizations
# Then reboot the system

Write-Output "Restoring DPC optimization defaults..."

# === ROUND 1: MMCSS and Visual Effects ===

# MMCSS SystemResponsiveness back to default (20)
Set-ItemProperty -Path "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile" -Name SystemResponsiveness -Value 20 -Type DWord
Write-Output "MMCSS SystemResponsiveness restored to 20"

# MMCSS Pro Audio - restore defaults
Set-ItemProperty -Path "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Pro Audio" -Name "Clock Rate" -Value 10000 -Type DWord
Set-ItemProperty -Path "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Pro Audio" -Name "GPU Priority" -Value 8 -Type DWord
Set-ItemProperty -Path "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Pro Audio" -Name Priority -Value 6 -Type DWord
Set-ItemProperty -Path "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Pro Audio" -Name "SFIO Priority" -Value "High" -Type String
Remove-ItemProperty -Path "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Pro Audio" -Name Affinity -ErrorAction SilentlyContinue
Write-Output "MMCSS Pro Audio restored to defaults"

# Re-enable Transparency for ableton-pc user
$sid = "S-1-5-21-15251608-2992027975-817598959-1001"
reg load HKU\TempUser "C:\Users\ableton-pc\NTUSER.DAT" 2>$null
Set-ItemProperty -Path "Registry::HKU\$sid\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize" -Name EnableTransparency -Value 1 -Type DWord -ErrorAction SilentlyContinue
Set-ItemProperty -Path "Registry::HKU\TempUser\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize" -Name EnableTransparency -Value 1 -Type DWord -ErrorAction SilentlyContinue
Write-Output "Transparency re-enabled"

# Restore Visual Effects
Set-ItemProperty -Path "Registry::HKU\$sid\Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects" -Name VisualFXSetting -Value 0 -Type DWord -ErrorAction SilentlyContinue
Set-ItemProperty -Path "Registry::HKU\$sid\Control Panel\Desktop" -Name MinAnimate -Value "1" -Type String -ErrorAction SilentlyContinue
Set-ItemProperty -Path "Registry::HKU\$sid\Control Panel\Desktop" -Name DragFullWindows -Value "1" -Type String -ErrorAction SilentlyContinue
Set-ItemProperty -Path "Registry::HKU\$sid\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced" -Name TaskbarAnimations -Value 1 -Type DWord -ErrorAction SilentlyContinue
Write-Output "Visual effects restored to defaults"

Write-Output "NOTE: AMD HD Audio and Streaming Audio devices need manual re-enable via Device Manager"

# === ROUND 2: Interrupt Affinity and GPU ===

# Yamaha AIC128-D - remove custom interrupt affinity
# Device: PCI\VEN_1A39&DEV_0004&SUBSYS_E04E1A39&REV_01\6&291EA6AB&0&0020020A
$yamahaPath = "HKLM:\SYSTEM\CurrentControlSet\Enum\PCI\VEN_1A39&DEV_0004&SUBSYS_E04E1A39&REV_01\6&291EA6AB&0&0020020A\Device Parameters\Interrupt Management\Affinity Policy"
if (Test-Path $yamahaPath) {
    Remove-ItemProperty -Path $yamahaPath -Name DevicePolicy -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path $yamahaPath -Name AssignmentSetOverride -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path $yamahaPath -Name DevicePriority -ErrorAction SilentlyContinue
    Write-Output "Yamaha interrupt affinity removed (will use Windows defaults)"
}

# AMD Radeon RX 5500 XT - remove custom interrupt affinity and priority
# Device: PCI\VEN_1002&DEV_7340&SUBSYS_38221462&REV_C5\6&18F95EC&0&00000019
$gpuPath = "HKLM:\SYSTEM\CurrentControlSet\Enum\PCI\VEN_1002&DEV_7340&SUBSYS_38221462&REV_C5\6&18F95EC&0&00000019\Device Parameters\Interrupt Management\Affinity Policy"
if (Test-Path $gpuPath) {
    Remove-ItemProperty -Path $gpuPath -Name DevicePolicy -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path $gpuPath -Name AssignmentSetOverride -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path $gpuPath -Name DevicePriority -ErrorAction SilentlyContinue
    Write-Output "GPU interrupt affinity removed (will use Windows defaults)"
}

# HW GPU Scheduling - re-enable (default = 2)
Set-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers" -Name HwSchMode -Value 2 -Type DWord
Write-Output "HW GPU Scheduling re-enabled"

# AMD ULPS - re-enable
Get-ChildItem "HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}" -ErrorAction SilentlyContinue | ForEach-Object {
    Set-ItemProperty -Path $_.PSPath -Name EnableULPS -Value 1 -Type DWord -ErrorAction SilentlyContinue
}
Write-Output "AMD ULPS re-enabled"

# BCDEdit - restore defaults
bcdedit /deletevalue disabledynamictick 2>$null
bcdedit /deletevalue useplatformtick 2>$null
bcdedit /deletevalue tscsyncpolicy 2>$null
Write-Output "BCDEdit timer settings restored to defaults"

# PP_SclkDeepSleepDisable - restore
Get-ChildItem "HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}" -ErrorAction SilentlyContinue | ForEach-Object {
    Set-ItemProperty -Path $_.PSPath -Name PP_SclkDeepSleepDisable -Value 0 -Type DWord -ErrorAction SilentlyContinue
}
Write-Output "GPU deep sleep restored"

# CrossFire - remove
Get-ChildItem "HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}" -ErrorAction SilentlyContinue | ForEach-Object {
    Remove-ItemProperty -Path $_.PSPath -Name EnableCrossFireAutoLink -ErrorAction SilentlyContinue
}
Write-Output "CrossFire setting removed"

# PCIe ASPM - restore to default (moderate savings)
powercfg /setacvalueindex SCHEME_CURRENT SUB_PCIEXPRESS ASPM 2
powercfg /setdcvalueindex SCHEME_CURRENT SUB_PCIEXPRESS ASPM 2
powercfg /setactive SCHEME_CURRENT
Write-Output "PCIe ASPM restored to default"

# === ROUND 3: Aggressive GPU/DWM fixes ===

# GPU power/preemption - remove
Get-ChildItem "HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}" -ErrorAction SilentlyContinue | ForEach-Object {
    Remove-ItemProperty -Path $_.PSPath -Name DisablePowerGating -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path $_.PSPath -Name DisableDPM -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path $_.PSPath -Name KMD_EnableComputePreemption -ErrorAction SilentlyContinue
}
Write-Output "GPU power gating, DPM, compute preemption restored to defaults"

# DWM - restore
Remove-ItemProperty -Path "HKLM:\SOFTWARE\Microsoft\Windows\DWM" -Name DisableFlipExModel -ErrorAction SilentlyContinue
Remove-ItemProperty -Path "HKLM:\SOFTWARE\Microsoft\Windows\DWM" -Name OverlayTestMode -ErrorAction SilentlyContinue
Write-Output "DWM settings restored to defaults"

# GameDVR - restore defaults
New-Item -Path "HKLM:\SOFTWARE\Policies\Microsoft\Windows\GameDVR" -Force -ErrorAction SilentlyContinue | Out-Null
Set-ItemProperty -Path "HKLM:\SOFTWARE\Policies\Microsoft\Windows\GameDVR" -Name AllowGameDVR -Value 1 -Type DWord -ErrorAction SilentlyContinue
Set-ItemProperty -Path "Registry::HKU\$sid\SOFTWARE\Microsoft\Windows\CurrentVersion\GameDVR" -Name AppCaptureEnabled -Value 1 -Type DWord -ErrorAction SilentlyContinue
Set-ItemProperty -Path "Registry::HKU\$sid\System\GameConfigStore" -Name GameDVR_Enabled -Value 1 -Type DWord -ErrorAction SilentlyContinue
Write-Output "GameDVR restored to defaults"

reg unload HKU\TempUser 2>$null

Write-Output ""
Write-Output "=== ALL CHANGES RESTORED ==="
Write-Output "REBOOT REQUIRED for changes to take effect"
Write-Output "Run: shutdown /r /t 5 /f"
