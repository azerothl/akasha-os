# install-windows-service.ps1 — opt-in logon scheduled task for aos-serverd (P21.5)
#
# ADR 0012: prefer a *user* logon task over Session 0 Windows Service (GPU /
# interactive desktop limits). Default Preview install does NOT run this.
# Desktop shortcut → aos-session remains the default path.
param(
    [string]$Prefix = $(if ($env:AOS_HOME) { $env:AOS_HOME } else { Join-Path $env:LOCALAPPDATA "AgentOS-Preview" }),
    [string]$TaskName = "AkashaOS-Preview-aos-serverd",
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
$here = $PSScriptRoot

if ($Uninstall) {
    $existing = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    if ($existing) {
        Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
        Write-Host "OK. Removed scheduled task '$TaskName'."
    } else {
        Write-Host "No scheduled task '$TaskName' found."
    }
    return
}

$binCandidates = @(
    (Join-Path $here "bin\aos-serverd.exe"),
    (Join-Path $Prefix "bin\aos-serverd.exe")
)
$exe = $binCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $exe) {
    throw "aos-serverd.exe not found under $here\bin or $Prefix\bin. Run install.ps1 first."
}

New-Item -ItemType Directory -Force -Path (Join-Path $Prefix "var\run") | Out-Null

$action = New-ScheduledTaskAction `
    -Execute $exe `
    -Argument "--aos-home `"$Prefix`" serve" `
    -WorkingDirectory $Prefix
# At logon for the current user — not Session 0.
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
$settings = New-ScheduledTaskSettingsSet `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries `
    -StartWhenAvailable `
    -RestartCount 3 `
    -RestartInterval (New-TimeSpan -Minutes 1) `
    -ExecutionTimeLimit ([TimeSpan]::Zero)
$principal = New-ScheduledTaskPrincipal `
    -UserId $env:USERNAME `
    -LogonType Interactive `
    -RunLevel Limited

$existing = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if ($existing) {
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
}

Register-ScheduledTask `
    -TaskName $TaskName `
    -Action $action `
    -Trigger $trigger `
    -Settings $settings `
    -Principal $principal `
    -Description "Akasha OS Preview aos-serverd (headless lifecycle owner, P21.5). Opt-in; local control only." `
    | Out-Null

# Start immediately so the tree is up without waiting for next logon.
Start-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue

Write-Host "OK. Scheduled task registered: $TaskName"
Write-Host "  AOS_HOME=$Prefix"
Write-Host "  Execute=$exe serve"
Write-Host "Status : Get-ScheduledTask -TaskName '$TaskName'"
Write-Host "Control: & '$exe' --aos-home '$Prefix' status"
Write-Host "Remove : powershell -File .\install-windows-service.ps1 -Uninstall"
