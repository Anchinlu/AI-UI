$ErrorActionPreference = "Stop"

$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$csvFile = "E:\UI AI\ai-taskbar\Docs\benchmarks\raw\h7-6r_${timestamp}.csv"

$headers = "timestamp,backend,model,model_hash,quantization,binary_version,num_ctx,num_predict,temperature,repeat_penalty,run_type,run_index,prompt_tokens,generated_tokens,ttft_ms,total_latency_ms,generation_tokens_per_sec,process_id,process_cpu_time_ms,process_working_set_mb,status,error"
Set-Content -Path $csvFile -Value $headers -Encoding UTF8

$global:benchResults = @()

function Log-Result {
    param (
        $backend, $model, $hash, $quant, $binVer, $ctx, $predict, $temp, $penalty,
        $runType, $runIndex, $pTokens, $gTokens, $ttft, $totalLat, $tps,
        $procId, $cpuTime, $wsMB, $status, $errorMsg
    )
    $now = Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ"
    $binVer = $binVer -replace '`"', '""'
    $errorMsg = $errorMsg -replace '`"', '""'
    
    $line = "`"$now`",`"$backend`",`"$model`",`"$hash`",`"$quant`",`"$binVer`",$ctx,$predict,$temp,$penalty,`"$runType`",$runIndex,$pTokens,$gTokens,$ttft,$totalLat,$tps,$procId,$cpuTime,$wsMB,`"$status`",`"$errorMsg`""
    Add-Content -Path $csvFile -Value $line -Encoding UTF8
    
    $resultObj = [PSCustomObject]@{
        backend = $backend
        run_type = $runType
        run_index = $runIndex
        tps = $tps
        ttft = $ttft
        total_latency = $totalLat
        cpu = $cpuTime
        ram = $wsMB
        status = $status
    }
    $global:benchResults += $resultObj
}

function Save-Summary {
    param ($backend, $commandStr)
    $backendResults = $global:benchResults | Where-Object { $_.backend -eq $backend }
    if ($backendResults.Count -eq 0) { return }

    $cold = $backendResults | Where-Object { $_.run_type -eq "cold" }
    $warms = $backendResults | Where-Object { $_.run_type -eq "warm" -and $_.status -eq "OK" }
    $failed = $backendResults | Where-Object { $_.status -ne "OK" }

    $median_tps = 0
    $median_lat = 0
    $median_cpu = 0
    $median_ram = 0

    if ($warms.Count -gt 0) {
        $sortedTps = $warms | Sort-Object tps
        $median_tps = $sortedTps[[math]::Floor($sortedTps.Count / 2)].tps
        
        $sortedLat = $warms | Sort-Object total_latency
        $median_lat = $sortedLat[[math]::Floor($sortedLat.Count / 2)].total_latency
        
        $sortedCpu = $warms | Sort-Object cpu
        $median_cpu = $sortedCpu[[math]::Floor($sortedCpu.Count / 2)].cpu
        
        $sortedRam = $warms | Sort-Object ram
        $median_ram = $sortedRam[[math]::Floor($sortedRam.Count / 2)].ram
    }

    $summary = @{
        backend = $backend
        command = $commandStr
        cold_start = $cold
        warm_runs = $warms
        median_generation_tokens_per_sec = $median_tps
        median_total_latency_ms = $median_lat
        median_process_cpu_time_ms = $median_cpu
        median_process_working_set_mb = $median_ram
        failed_runs = $failed
    }
    
    $jsonFile = "E:\UI AI\ai-taskbar\Docs\benchmarks\raw\h7-6r_${timestamp}_${backend}_summary.json"
    $summary | ConvertTo-Json -Depth 5 | Set-Content -Path $jsonFile -Encoding UTF8
}

function Get-Inference-Process {
    param($backend)
    if ($backend -eq "ollama") {
        # Ollama spawns ollama_llama_server.exe for inference
        $proc = Get-Process ollama_llama_server -ErrorAction SilentlyContinue | Sort-Object CPU -Descending | Select-Object -First 1
        return $proc
    } else {
        # llama.cpp is llama-server.exe
        $proc = Get-Process llama-server -ErrorAction SilentlyContinue | Sort-Object CPU -Descending | Select-Object -First 1
        return $proc
    }
}

# --- CONFIG ---
$modelStr = "qwen2.5:1.5b"
$hash = "sha256-183715c435899236895da3869489cc30ac241476b4971a20285b1a462818a5b4"
$quant = "Q4_K_M"
$numCtx = 2048
$numPredict = 64
$temperature = 0.0
$repeatPenalty = 1.1
$promptText = "Xin chào. Hãy giới thiệu ngắn gọn về chính bạn trong khoảng 50 từ."
$warmRuns = 5

# --- 1. OLLAMA ---
Write-Host "--- Starting Ollama Benchmark ---"
Stop-Process -Name ollama -Force -ErrorAction SilentlyContinue
Stop-Process -Name ollama_llama_server -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 3

$ollamaBinVer = (ollama -v) -replace "`n", ""
$ollamaCmd = "ollama serve"
$env:OLLAMA_HOST = "127.0.0.1:11434"
$env:OLLAMA_MODELS = "C:\Users\lctan\.ollama\models"
$ollamaProc = Start-Process -FilePath "ollama" -ArgumentList "serve" -NoNewWindow -PassThru
Start-Sleep -Seconds 5

$ollamaUrl = "http://127.0.0.1:11434/api/chat"
$ollamaBody = @{
    model = $modelStr
    messages = @( @{ role = "user"; content = $promptText } )
    stream = $false
    options = @{
        temperature = $temperature
        num_predict = $numPredict
        repeat_penalty = $repeatPenalty
        num_ctx = $numCtx
    }
} | ConvertTo-Json -Depth 5

for ($i = 0; $i -le $warmRuns; $i++) {
    $runType = if ($i -eq 0) { "cold" } else { "warm" }
    Write-Host "Ollama: Running $runType run $i..."
    
    try {
        $infProcBefore = Get-Inference-Process -backend "ollama"
        $cpuBefore = if ($infProcBefore) { $infProcBefore.CPU } else { 0 }
        
        $sw = [Diagnostics.Stopwatch]::StartNew()
        $res = Invoke-RestMethod -Uri $ollamaUrl -Method Post -Body $ollamaBody -ContentType "application/json; charset=utf-8" -ErrorAction Stop
        $sw.Stop()
        
        $infProcAfter = Get-Inference-Process -backend "ollama"
        $cpuAfter = if ($infProcAfter) { $infProcAfter.CPU } else { 0 }
        
        $pidTarget = if ($infProcAfter) { $infProcAfter.Id } else { 0 }
        $cpuMs = if ($infProcAfter -and $infProcBefore) { [math]::Round(($cpuAfter - $cpuBefore) * 1000, 2) } else { 0 }
        $wsMB = if ($infProcAfter) { [math]::Round($infProcAfter.WorkingSet64 / 1MB, 2) } else { 0 }
        
        $pTokens = $res.prompt_eval_count
        $gTokens = $res.eval_count
        $ttft = [math]::Round($res.prompt_eval_duration / 1000000.0, 2)
        $totalLat = [math]::Round($res.total_duration / 1000000.0, 2)
        $tps = if ($res.eval_duration -gt 0) { [math]::Round($gTokens / ($res.eval_duration / 1000000000.0), 2) } else { 0 }
        
        Log-Result "ollama" $modelStr $hash $quant $ollamaBinVer $numCtx $numPredict $temperature $repeatPenalty $runType $i $pTokens $gTokens $ttft $totalLat $tps $pidTarget $cpuMs $wsMB "OK" ""
    } catch {
        Log-Result "ollama" $modelStr $hash $quant $ollamaBinVer $numCtx $numPredict $temperature $repeatPenalty $runType $i 0 0 0 0 0 0 0 0 "ERROR" $_.Exception.Message
    }
    Start-Sleep -Seconds 1
}

Save-Summary "ollama" $ollamaCmd
Stop-Process -Name ollama -Force -ErrorAction SilentlyContinue
Stop-Process -Name ollama_llama_server -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 3

# --- 2. LLAMA.CPP ---
Write-Host "--- Starting llama.cpp Benchmark ---"
$llamaExe = "E:\UI AI\ai-taskbar\llama-cpp\llama-server.exe"
try {
    $llamaBinVer = (& cmd.exe /c "`"$llamaExe`" --version 2>&1") | Select-Object -First 1
    $llamaBinVer = $llamaBinVer -replace "`n", ""
} catch {
    $llamaBinVer = "unknown"
}
$modelFile = "C:\Users\lctan\.ollama\models\blobs\sha256-183715c435899236895da3869489cc30ac241476b4971a20285b1a462818a5b4"

$llamaCmd = "$llamaExe --host 127.0.0.1 --port 8081 -ngl 0 -c $numCtx -n $numPredict -m `"$modelFile`""
$llamaProc = Start-Process -FilePath $llamaExe -ArgumentList "--host 127.0.0.1 --port 8081 -ngl 0 -c $numCtx -n $numPredict -m `"$modelFile`"" -NoNewWindow -PassThru
Start-Sleep -Seconds 10 # Wait for model load

$llamaUrl = "http://127.0.0.1:8081/v1/chat/completions"
$llamaBody = @{
    model = $modelFile
    messages = @( @{ role = "user"; content = $promptText } )
    stream = $false
    temperature = $temperature
    n_predict = $numPredict
    repeat_penalty = $repeatPenalty
} | ConvertTo-Json -Depth 5

for ($i = 0; $i -le $warmRuns; $i++) {
    $runType = if ($i -eq 0) { "cold" } else { "warm" }
    Write-Host "llama.cpp: Running $runType run $i..."
    
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
        $totalLat = $sw.ElapsedMilliseconds
        # timings are omitted in chat completion unless options are passed, so we estimate total / we'll extract if possible.
        # Wait, openai compat doesn't return TTFT easily without stream.
        # But for benchmark contract, we do our best. llama.cpp does NOT return prompt_eval_duration in v1/chat/completions.
        $ttft = 0 # Cannot get from OpenAI spec accurately without parsing logs
        # Actually, sometimes llama.cpp adds timings to the response? Let's check if it exists:
        if ($null -ne $res.timings) {
            $ttft = [math]::Round($res.timings.prompt_ms, 2)
            $tps = [math]::Round($res.timings.predicted_per_second, 2)
        } else {
            $tps = if ($totalLat -gt 0) { [math]::Round($gTokens / ($totalLat / 1000.0), 2) } else { 0 }
        }
        
        Log-Result "llamacpp" $modelStr $hash $quant $llamaBinVer $numCtx $numPredict $temperature $repeatPenalty $runType $i $pTokens $gTokens $ttft $totalLat $tps $pidTarget $cpuMs $wsMB "OK" ""
    } catch {
        Log-Result "llamacpp" $modelStr $hash $quant $llamaBinVer $numCtx $numPredict $temperature $repeatPenalty $runType $i 0 0 0 0 0 0 0 0 "ERROR" $_.Exception.Message
    }
    Start-Sleep -Seconds 1
}

Save-Summary "llamacpp" $llamaCmd
Stop-Process -Id $llamaProc.Id -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 3

Write-Host "Benchmark completed. Results saved in Docs/benchmarks/raw/"
