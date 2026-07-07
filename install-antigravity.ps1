# Windows Installer for cheat-engine-mcp on Google Antigravity (PowerShell)

Write-Host "=== Installing cheat-engine-mcp for Google Antigravity (Windows) ===" -ForegroundColor Cyan

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

# Copy skill definition
$SkillDir = Join-Path $env:USERPROFILE ".gemini\config\skills\cheat-engine-mcp"
Write-Host "2. Copying skill to $SkillDir..." -ForegroundColor Yellow
if (!(Test-Path -Path $SkillDir)) {
    New-Item -ItemType Directory -Force -Path $SkillDir
}
Copy-Item -Path "skills\antigravity\SKILL.md" -Destination "$SkillDir\SKILL.md" -Force
Write-Host "installed: $SkillDir\SKILL.md" -ForegroundColor Green

# Configure settings.json
$SettingsFile = Join-Path $env:USERPROFILE ".gemini\config\settings.json"
Write-Host "3. Configuring $SettingsFile..." -ForegroundColor Yellow

if (!(Test-Path -Path $SettingsFile)) {
    $ParentDir = Split-Path -Path $SettingsFile
    if (!(Test-Path -Path $ParentDir)) {
        New-Item -ItemType Directory -Force -Path $ParentDir
    }
    '{"mcpServers": {}}' | Out-File -FilePath $SettingsFile -Encoding utf8
}

# Update settings json using PowerShell JSON parsing
$Config = Get-Content -Raw -Path $SettingsFile | ConvertFrom-Json
if ($null -eq $Config.mcpServers) {
    $Config | Add-Member -MemberType NoteProperty -Name "mcpServers" -Value (New-Object PSObject)
}

$McpServer = New-Object PSObject
$McpServer | Add-Member -MemberType NoteProperty -Name "command" -Value "C:\Tools\cheat-engine-mcp.exe"

if ($null -eq $Config.mcpServers."cheat-engine-mcp") {
    $Config.mcpServers | Add-Member -MemberType NoteProperty -Name "cheat-engine-mcp" -Value $McpServer
} else {
    $Config.mcpServers."cheat-engine-mcp" = $McpServer
}

$Config | ConvertTo-Json -Depth 10 | Out-File -FilePath $SettingsFile -Encoding utf8

Write-Host "=== Installation Completed Successfully! ===" -ForegroundColor Green
Write-Host "Restart your Antigravity agent and test by typing: 'ping cheat-engine-mcp'" -ForegroundColor Green
