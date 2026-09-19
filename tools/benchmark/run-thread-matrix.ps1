# run-thread-matrix.ps1
# Script chạy benchmark ma trận luồng (threads) cho llama.cpp (Mốc H7-7)
$ErrorActionPreference = "Stop"

# Thư mục lưu kết quả
$rawDir = "E:\UI AI\ai-taskbar\Docs\benchmarks\raw"
if (-not (Test-Path $rawDir)) {
    New-Item -ItemType Directory -Path $rawDir | Out-Null
}

$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$csvFile = Join-Path $rawDir "thread_matrix_$timestamp.csv"
$jsonFile = Join-Path $rawDir "thread_matrix_${timestamp}_summary.json"

# Khởi tạo file CSV
$csvHeader = "Timestamp,Backend,Threads,Model,Hash,Quantization,BinaryVer,Ctx,Predict,Temp,Penalty,RunType,RunIndex,PromptTokens,GenTokens,TTFT_ms,TotalLat_ms,TPS,ProcId,CpuDelta_ms,Ws_MB,Status,ErrorMsg"
Set-Content -Path $csvFile -Value $csvHeader -Encoding UTF8

$globalResults = @()

function Get-Inference-Process {
    param([string]$backend)
    
    if ($backend -eq "llamacpp") {
        $p = Get-Process -Name "llama-server" -ErrorAction SilentlyContinue
        if ($p) { return $p | Select-Object -First 1 }
    }
    return $null
}

function Log-Result {
    param (
        $backend, $threads, $model, $hash, $quant, $binVer, $ctx, $predict, $temp, $penalty,
        $runType, $runIndex, $pTokens, $gTokens, $ttft, $totalLat, $tps,
        $procId, $cpuTime, $wsMB, $status, $errorMsg
    )
    $now = Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ"
    $binVer = $binVer -replace '`"', '""'
    $errorMsg = $errorMsg -replace '`"', '""'
    
    $line = "`"$now`",`"$backend`",$threads,`"$model`",`"$hash`",`"$quant`",`"$binVer`",$ctx,$predict,$temp,$penalty,`"$runType`",$runIndex,$pTokens,$gTokens,$ttft,$totalLat,$tps,$procId,$cpuTime,$wsMB,`"$status`",`"$errorMsg`""
    Add-Content -Path $csvFile -Value $line -Encoding UTF8
    
    $resultObj = [PSCustomObject]@{
        backend = $backend; threads = $threads; run_type = $runType; run_index = $runIndex
        tps = $tps; ttft = $ttft; total_latency = $totalLat; cpu = $cpuTime; ram = $wsMB; status = $status
    }
    $global:globalResults += $resultObj
    return $resultObj
}

function Save-Summary {
    $summary = @{}
    $threadGroups = $global:globalResults | Group-Object threads
    foreach ($grp in $threadGroups) {
        $warmRuns = $grp.Group | Where-Object { $_.run_type -eq "warm" -and $_.status -eq "OK" }
        $coldRun = $grp.Group | Where-Object { $_.run_type -eq "cold" } | Select-Object -First 1
        
        $medianTps = 0; $medianLat = 0; $medianCpu = 0; $medianRam = 0
        if ($warmRuns.Count -gt 0) {
            $sortedTps = $warmRuns | Sort-Object tps
            $medianTps = $sortedTps[[math]::Floor($sortedTps.Count / 2)].tps
            
            $sortedLat = $warmRuns | Sort-Object total_latency
            $medianLat = $sortedLat[[math]::Floor($sortedLat.Count / 2)].total_latency
            
            $sortedCpu = $warmRuns | Sort-Object cpu
            $medianCpu = $sortedCpu[[math]::Floor($sortedCpu.Count / 2)].cpu
            
            $sortedRam = $warmRuns | Sort-Object ram
            $medianRam = $sortedRam[[math]::Floor($sortedRam.Count / 2)].ram
        }
        
        $summary[$grp.Name] = @{
            threads = [int]$grp.Name
            median_tps = $medianTps
            median_latency_ms = $medianLat
            median_cpu_delta_ms = $medianCpu
            median_ram_mb = $medianRam
            runs = $grp.Group
        }
    }
    $summary | ConvertTo-Json -Depth 5 | Set-Content -Path $jsonFile -Encoding UTF8
}

# --- Cấu hình đo ---
$modelName = "qwen2.5:1.5b"
$quantLevel = "Q4_K_M"
$modelFile = "C:\Users\lctan\.ollama\models\blobs\sha256-183715c435899236895da3869489cc30ac241476b4971a20285b1a462818a5b4"
$modelHash = "sha256-183715c435899236895da3869489cc30ac241476b4971a20285b1a462818a5b4"

$promptText = "Xin chào. Hãy giới thiệu ngắn gọn về chính bạn trong khoảng 50 từ."
$temperature = 0.0
$numCtx = 2048
$numPredict = 64
$repeatPenalty = 1.1

$llamaExe = "E:\UI AI\ai-taskbar\llama-cpp\llama-server.exe"
try {
    $llamaBinVer = (& cmd.exe /c "`"$llamaExe`" --version 2>&1") | Select-Object -First 1
    $llamaBinVer = $llamaBinVer -replace "`n", ""
} catch {
    $llamaBinVer = "unknown"
}

$llamaBody = @{
    model = $modelFile
    messages = @( @{ role = "user"; content = $promptText } )
    stream = $false
    temperature = $temperature
    max_tokens = $numPredict
    repeat_penalty = $repeatPenalty
} | ConvertTo-Json -Depth 5

$threadList = @(2, 4, 6, 8, 10, 12)

Write-Host "Starting thread matrix benchmark for llama.cpp..."
foreach ($t in $threadList) {
    Write-Host "`n==============================================="
    Write-Host "Testing config: $t Threads"
    Write-Host "==============================================="
    
    # Kill process cũ
    Get-Process -Name "llama-server" -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Seconds 2
    
    $llamaCmd = "$llamaExe --host 127.0.0.1 --port 8081 -ngl 0 -c $numCtx -n $numPredict -t $t -tb $t -m `"$modelFile`""
    $llamaProc = Start-Process -FilePath $llamaExe -ArgumentList "--host 127.0.0.1 --port 8081 -ngl 0 -c $numCtx -n $numPredict -t $t -tb $t -m `"$modelFile`"" -NoNewWindow -PassThru
    
    # Đợi server lên
    Start-Sleep -Seconds 12
    $llamaUrl = "http://127.0.0.1:8081/v1/chat/completions"
    
    # 1 Cold run, 3 Warm runs
    for ($i = 0; $i -le 3; $i++) {
        $runType = if ($i -eq 0) { "cold" } else { "warm" }
        Write-Host "Thread $t - Running $runType run $i..."
        
        try {
            $infProcBefore = Get-Inference-Process -backend "llamacpp"
            $cpuBefore = if ($infProcBefore) { $infProcBefore.CPU } else { 0 }

            $sw = [Diagnostics.Stopwatch]::StartNew()
            $res = Invoke-RestMethod -Uri $llamaUrl -Method Post -Body $llamaBody -ContentType "application/json; charset=utf-8" -ErrorAction Stop
            $sw.Stop()
            
            $infProcAfter = Get-Inference-Process -backend "llamacpp"
            $cpuAfter = if ($infProcAfter) { $infProcAfter.CPU } else { 0 }
            
            $pidTarget = if ($infProcAfter) { $infProcAfter.Id } else { 0 }
            $cpuMs = if ($infProcAfter -and $infProcBefore) { [math]::Round(($cpuAfter - $cpuBefore) * 1000, 2) } else { 0 }
            $wsMB = if ($infProcAfter) { [math]::Round($infProcAfter.WorkingSet64 / 1MB, 2) } else { 0 }
            
            $pTokens = $res.usage.prompt_tokens
            $gTokens = $res.usage.completion_tokens
            $ttft = 0 # stream=false -> no TTFT
            $totalLat = [math]::Round($sw.Elapsed.TotalMilliseconds, 2)
            $tps = if ($totalLat -gt 0) { [math]::Round(($gTokens / ($totalLat / 1000)), 2) } else { 0 }
            
            Log-Result -backend "llamacpp" -threads $t -model $modelName -hash $modelHash -quant $quantLevel -binVer $llamaBinVer `
                -ctx $numCtx -predict $numPredict -temp $temperature -penalty $repeatPenalty `
                -runType $runType -runIndex $i -pTokens $pTokens -gTokens $gTokens -ttft $ttft -totalLat $totalLat -tps $tps `
                -procId $pidTarget -cpuTime $cpuMs -wsMB $wsMB -status "OK" -errorMsg ""
        } catch {
            $errMsg = $_.Exception.Message
            Write-Host "Error: $errMsg" -ForegroundColor Red
            Log-Result -backend "llamacpp" -threads $t -model $modelName -hash $modelHash -quant $quantLevel -binVer $llamaBinVer `
                -ctx $numCtx -predict $numPredict -temp $temperature -penalty $repeatPenalty `
                -runType $runType -runIndex $i -pTokens 0 -gTokens 0 -ttft 0 -totalLat 0 -tps 0 `
                -procId 0 -cpuTime 0 -wsMB 0 -status "ERROR" -errorMsg $errMsg
        }
        
        Start-Sleep -Seconds 3
    }
    
    Get-Process -Name "llama-server" -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Seconds 2
}

Save-Summary
Write-Host "Benchmark completed. Data saved at $rawDir"
