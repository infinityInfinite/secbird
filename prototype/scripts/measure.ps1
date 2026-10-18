param(
    [Parameter(Mandatory=$true)][int]$RootProcessId,
    [Parameter(Mandatory=$true)][ValidateSet('open-idle','tray-hidden','interface-destroyed')][string]$State,
    [int]$Seconds = 60,
    [string]$OutputDirectory = "$PSScriptRoot/../measurements"
)
$ErrorActionPreference = 'Stop'
if ($Seconds -lt 2) { throw 'Seconds doit être >= 2.' }
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$logicalCores = [Environment]::ProcessorCount
$root = Get-Process -Id $RootProcessId
$rootStart = $root.StartTime.ToUniversalTime()
$known = @{}
$known[$RootProcessId] = $rootStart
$samples = @()
$previousCpu = @{}
$watch = [Diagnostics.Stopwatch]::StartNew()
$previousTime = 0.0
for ($index = 0; $index -le $Seconds; $index++) {
    $processRows = Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name,CreationDate
    $changed = $true
    while ($changed) {
        $changed = $false
        foreach ($row in $processRows) {
            $id = [int]$row.ProcessId
            $parentId = [int]$row.ParentProcessId
            if (!$known.ContainsKey($id) -and $known.ContainsKey($parentId) -and $row.CreationDate.ToUniversalTime() -ge $known[$parentId]) {
                $known[$id] = $row.CreationDate.ToUniversalTime()
                $changed = $true
            }
        }
    }
    $totalWs = 0L; $totalPrivate = 0L; $cpuDelta = 0.0; $members = @()
    foreach ($row in $processRows) {
        $id = [int]$row.ProcessId
        if (!$known.ContainsKey($id)) { continue }
        try {
            $proc = Get-Process -Id $id -ErrorAction Stop
            if ([Math]::Abs(($proc.StartTime.ToUniversalTime() - $known[$id]).TotalSeconds) -gt 1) { continue }
            $cpu = $proc.TotalProcessorTime.TotalSeconds
            if ($previousCpu.ContainsKey($id)) { $cpuDelta += [Math]::Max(0, $cpu - $previousCpu[$id]) }
            $previousCpu[$id] = $cpu
            $totalWs += $proc.WorkingSet64
            $totalPrivate += $proc.PrivateMemorySize64
            $members += @{pid=$id;name=$proc.ProcessName;workingSetBytes=$proc.WorkingSet64;privateBytes=$proc.PrivateMemorySize64}
        } catch { }
    }
    $elapsed = $watch.Elapsed.TotalSeconds
    if ($index -gt 0) {
        $interval = $elapsed - $previousTime
        $samples += @{seconds=$elapsed;workingSetBytes=$totalWs;privateBytes=$totalPrivate;cpuOneCorePercent=100*$cpuDelta/$interval;cpuMachinePercent=100*$cpuDelta/$interval/$logicalCores;processes=$members}
    }
    $previousTime = $elapsed
    if ($index -lt $Seconds) { Start-Sleep -Seconds 1 }
}
function Percentile($Values, [double]$Fraction) {
    $sorted = @($Values | Sort-Object)
    return $sorted[[Math]::Min($sorted.Count-1, [Math]::Ceiling($sorted.Count*$Fraction)-1)]
}
$result = @{
    state=$State;timestampUtc=[DateTime]::UtcNow.ToString('o');rootProcessId=$RootProcessId;logicalCores=$logicalCores;
    rootStartUtc=$rootStart.ToString('o');os=[Environment]::OSVersion.VersionString;
    note='Somme parent + descendants, y compris WebView2. Working set additionne aussi les pages partagées. CPU des processus nouvellement créés/exités entre échantillons non entièrement capturé. État interface-destroyed de la démo : aucun moteur matériel actif.';
    summary=@{
        workingSetMedianMiB=(Percentile @($samples.workingSetBytes) 0.5)/1MB;
        workingSetP95MiB=(Percentile @($samples.workingSetBytes) 0.95)/1MB;
        privateMedianMiB=(Percentile @($samples.privateBytes) 0.5)/1MB;
        privateP95MiB=(Percentile @($samples.privateBytes) 0.95)/1MB;
        cpuMachineMedianPercent=Percentile @($samples.cpuMachinePercent) 0.5;
        cpuMachineP95Percent=Percentile @($samples.cpuMachinePercent) 0.95
    };samples=$samples
}
$outputPath = Join-Path $OutputDirectory ($State+'-'+[DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss')+'.json')
$result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $outputPath -Encoding utf8
$result.summary | Format-List
Write-Output "Relevé : $outputPath"
