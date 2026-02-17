#!/usr/bin/env pwsh

# Configuration
$Repo = "meet447/Enuma"
$BinaryName = "Enuma"
$InstallDir = if ($env:INSTALL_DIR) { $env:INSTALL_DIR } else { "$env:LOCALAPPDATA\Programs" }

# Detect architecture
function Get-Platform {
    $Arch = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture
    
    switch ($Arch) {
        "X64" { return "x86_64-pc-windows-msvc" }
        "Arm64" { return "aarch64-pc-windows-msvc" }
        default { return "unsupported" }
    }
}

# Get latest release version
function Get-LatestVersion {
    try {
        $Response = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -UseBasicParsing
        return $Response.tag_name
    } catch {
        return $null
    }
}

# Main installation
function Install-Enuma {
    Write-Host "Installing $BinaryName..." -ForegroundColor Green
    
    # Detect platform
    $Platform = Get-Platform
    if ($Platform -eq "unsupported") {
        Write-Host "Error: Unsupported platform" -ForegroundColor Red
        Write-Host "Supported platforms: Windows (x86_64, aarch64)"
        exit 1
    }
    
    Write-Host "Detected platform: $Platform"
    
    # Get latest version
    $Version = Get-LatestVersion
    if (-not $Version) {
        Write-Host "Error: Could not determine latest version" -ForegroundColor Red
        exit 1
    }
    
    Write-Host "Latest version: $Version"
    
    # Create download URL
    $DownloadUrl = "https://github.com/$Repo/releases/download/$Version/${BinaryName}-${Platform}.zip"
    
    # Create temp directory
    $TempDir = Join-Path $env:TEMP "enuma-install-$(Get-Random)"
    New-Item -ItemType Directory -Path $TempDir -Force | Out-Null
    
    try {
        # Download binary
        Write-Host "Downloading from: $DownloadUrl"
        $ZipPath = Join-Path $TempDir "${BinaryName}.zip"
        
        try {
            Invoke-WebRequest -Uri $DownloadUrl -OutFile $ZipPath -UseBasicParsing
        } catch {
            # Try without .zip extension
            $DownloadUrl = "https://github.com/$Repo/releases/download/$Version/${BinaryName}-${Platform}.exe"
            $ExePath = Join-Path $TempDir "${BinaryName}.exe"
            Invoke-WebRequest -Uri $DownloadUrl -OutFile $ExePath -UseBasicParsing
        }
        
        # Extract if zip
        if (Test-Path $ZipPath) {
            Expand-Archive -Path $ZipPath -DestinationPath $TempDir -Force
        }
        
        # Create install directory if it doesn't exist
        if (-not (Test-Path $InstallDir)) {
            New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
        }
        
        # Install binary
        $BinaryPath = Join-Path $TempDir "${BinaryName}.exe"
        $Destination = Join-Path $InstallDir "${BinaryName}.exe"
        
        Write-Host "Installing to $Destination" -ForegroundColor Yellow
        Move-Item -Path $BinaryPath -Destination $Destination -Force
        
        # Add to PATH if not already there
        $CurrentPath = [Environment]::GetEnvironmentVariable("Path", "User")
        if ($CurrentPath -notlike "*$InstallDir*") {
            Write-Host "Adding $InstallDir to your PATH..." -ForegroundColor Yellow
            [Environment]::SetEnvironmentVariable("Path", "$CurrentPath;$InstallDir", "User")
            Write-Host "Please restart your terminal for PATH changes to take effect" -ForegroundColor Yellow
        }
        
        Write-Host "✓ $BinaryName installed successfully!" -ForegroundColor Green
        Write-Host ""
        Write-Host "Run '$BinaryName --help' to get started"
        
    } finally {
        # Cleanup
        Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# Check for custom install directory
if ($args.Count -gt 0) {
    $InstallDir = $args[0]
}

# Alternative: install to cargo bin if it exists
$CargoBin = "$env:USERPROFILE\.cargo\bin"
if (Test-Path $CargoBin) {
    $InstallDir = $CargoBin
}

Install-Enuma
