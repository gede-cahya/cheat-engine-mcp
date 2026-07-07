# Windows Installer for cheat-engine-mcp on Claude Code (PowerShell)

Write-Host "=== Installing cheat-engine-mcp for Claude Code (Windows) ===" -ForegroundColor Cyan

# Build the release binary
Write-Host "1. Building release binary..." -ForegroundColor Yellow
cargo build --release

# Install binary to C:\Tools\
$BinDir = "C:\Tools"
if (!(Test-Path -Path $BinDir)) {
    New-Item -ItemType Directory -Force -Path $BinDir
}
Copy-Item -Path "target\release\cheat-engine-mcp.exe" -Destination "$BinDir\cheat-engine-mcp.exe" -Force
Write-Host "installed: $BinDir\cheat-engine-mcp.exe" -ForegroundColor Green

# Register with Claude Code CLI
Write-Host "2. Registering with Claude Code..." -ForegroundColor Yellow
if (Get-Command "claude" -ErrorAction SilentlyContinue) {
    claude mcp add cheat-engine-mcp "$BinDir\cheat-engine-mcp.exe"
    Write-Host "Registered with Claude Code successfully!" -ForegroundColor Green
} else {
    Write-Host "WARNING: 'claude' command not found. You will need to manually add it:" -ForegroundColor Yellow
    Write-Host "claude mcp add cheat-engine-mcp $BinDir\cheat-engine-mcp.exe" -ForegroundColor Yellow
}

Write-Host "=== Installation Completed! ===" -ForegroundColor Green
Write-Host "To copy Claude Code skills to your active workspace, run:" -ForegroundColor Green
Write-Host "mkdir YOUR_TARGET_REPO\.claude\skills" -ForegroundColor Yellow
Write-Host "Copy-Item -Recurse .claude\skills\cheat-engine-mcp YOUR_TARGET_REPO\.claude\skills" -ForegroundColor Yellow
Write-Host "Copy-Item CLAUDE.md YOUR_TARGET_REPO\CLAUDE.md" -ForegroundColor Yellow
