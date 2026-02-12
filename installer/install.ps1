# Audiotester Windows Installer
# Usage: irm https://raw.githubusercontent.com/newlevel/audiotester/main/installer/install.ps1 | iex

#Requires -Version 5.1

param(
    [string]$Version = "latest",
    [string]$InstallDir = "$env:LOCALAPPDATA\Audiotester",
    [switch]$NoShortcut,
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$RepoOwner = "newlevel"
$RepoName = "audiotester"
$ExeName = "audiotester.exe"

function Write-Status {
    param([string]$Message, [string]$Type = "Info")
    $colors = @{
        "Info"    = "Cyan"
        "Success" = "Green"
        "Warning" = "Yellow"
        "Error"   = "Red"
    }
    Write-Host "[$Type] $Message" -ForegroundColor $colors[$Type]
}

function Get-LatestVersion {
    try {
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$RepoOwner/$RepoName/releases/latest"
        return $release.tag_name -replace '^v', ''
    }
    catch {
        throw "Failed to fetch latest version: $_"
    }
}

function Get-ReleaseAssetUrl {
    param([string]$Version)

    $tagName = "v$Version"
    $assetName = "audiotester-$Version-windows-x64.zip"

    try {
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$RepoOwner/$RepoName/releases/tags/$tagName"
        $asset = $release.assets | Where-Object { $_.name -eq $assetName }

        if (-not $asset) {
            throw "Asset $assetName not found in release $tagName"
        }

        return $asset.browser_download_url
    }
    catch {
        throw "Failed to get release asset: $_"
    }
}

function Install-Audiotester {
    Write-Status "Audiotester Installer" "Info"
    Write-Host ""

    # Determine version
    if ($Version -eq "latest") {
        Write-Status "Fetching latest version..." "Info"
        $Version = Get-LatestVersion
    }
    Write-Status "Installing version: $Version" "Info"

    # Create install directory
    if (-not (Test-Path $InstallDir)) {
        Write-Status "Creating install directory: $InstallDir" "Info"
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    }

    # Download release
    $downloadUrl = Get-ReleaseAssetUrl -Version $Version
    $zipPath = Join-Path $env:TEMP "audiotester-$Version.zip"

    Write-Status "Downloading from: $downloadUrl" "Info"
    Invoke-WebRequest -Uri $downloadUrl -OutFile $zipPath

    # Extract
    Write-Status "Extracting to: $InstallDir" "Info"
    Expand-Archive -Path $zipPath -DestinationPath $InstallDir -Force

    # Cleanup
    Remove-Item $zipPath -Force

    # Verify installation
    $exePath = Join-Path $InstallDir $ExeName
    if (-not (Test-Path $exePath)) {
        throw "Installation failed: $ExeName not found"
    }

    # Add to PATH (user level)
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($userPath -notlike "*$InstallDir*") {
        Write-Status "Adding to PATH..." "Info"
        [Environment]::SetEnvironmentVariable(
            "Path",
            "$userPath;$InstallDir",
            "User"
        )
    }

    # Create Start Menu shortcut
    if (-not $NoShortcut) {
        $shortcutPath = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Audiotester.lnk"
        Write-Status "Creating Start Menu shortcut..." "Info"

        $shell = New-Object -ComObject WScript.Shell
        $shortcut = $shell.CreateShortcut($shortcutPath)
        $shortcut.TargetPath = $exePath
        $shortcut.WorkingDirectory = $InstallDir
        $shortcut.Description = "ASIO Audio Testing Application"
        $shortcut.Save()
    }

    Write-Host ""
    Write-Status "Installation complete!" "Success"
    Write-Host ""
    Write-Host "  Installed to: $InstallDir" -ForegroundColor White
    Write-Host "  Version: $Version" -ForegroundColor White
    Write-Host ""
    Write-Host "  Run with: audiotester" -ForegroundColor Cyan
    Write-Host ""
    Write-Status "NOTE: You may need to restart your terminal for PATH changes to take effect." "Warning"
}

function Uninstall-Audiotester {
    Write-Status "Uninstalling Audiotester..." "Info"

    # Remove install directory
    if (Test-Path $InstallDir) {
        Write-Status "Removing: $InstallDir" "Info"
        Remove-Item $InstallDir -Recurse -Force
    }

    # Remove from PATH
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($userPath -like "*$InstallDir*") {
        Write-Status "Removing from PATH..." "Info"
        $newPath = ($userPath.Split(';') | Where-Object { $_ -ne $InstallDir }) -join ';'
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
    }

    # Remove shortcut
    $shortcutPath = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Audiotester.lnk"
    if (Test-Path $shortcutPath) {
        Write-Status "Removing shortcut..." "Info"
        Remove-Item $shortcutPath -Force
    }

    Write-Host ""
    Write-Status "Uninstall complete!" "Success"
}

# Main
try {
    if ($Uninstall) {
        Uninstall-Audiotester
    }
    else {
        Install-Audiotester
    }
}
catch {
    Write-Status "Error: $_" "Error"
    exit 1
}
