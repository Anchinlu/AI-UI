param(
    [Parameter(Mandatory=$false)][string]$Backend = "ollama",
    [Parameter(Mandatory=$false)][int]$Iterations = 5,
    [Parameter(Mandatory=$false)][string]$LlamaCppPath = "",
    [Parameter(Mandatory=$false)][string]$ModelPath = "",
    [Parameter(Mandatory=$false)][int]$Port = 11435,
    [Parameter(Mandatory=$false)][int]$Ngl = 0,
    [Parameter(Mandatory=$false)][string]$Model = ""
)

if ($Model -eq "") {
    $Model = [Environment]::GetEnvironmentVariable("OLLAMA_MODEL")
    if (-not $Model) {
        $Model = "qwen2.5:1.5b"
    }
}

$prompt = "Please explain the theory of relativity in exactly 300 words."
$speeds = @()

if ($Backend -eq "llama.cpp") {
    if (-not (Test-Path $LlamaCppPath)) {
        Write-Error "Error: llama-server.exe not found at $LlamaCppPath"
        exit 1
    }
    if (-not (Test-Path $ModelPath)) {
        Write-Error "Error: GGUF file not found at $ModelPath"
        exit 1
    }
    Write-Host "Starting llama.cpp server..."
    $serverProcess = Start-Process -FilePath $LlamaCppPath -ArgumentList "-m `"$ModelPath`" --port $Port -ngl $Ngl" -PassThru -NoNewWindow
    Start-Sleep -Seconds 5
}

Write-Host "Starting Benchmark Backend: $Backend ($Iterations iterations)..."

for ($i = 1; $i -le $Iterations; $i++) {
    Write-Host "Run ${i}..."
    
    $body = @{
        prompt = $prompt
        stream = $false
    }
    
    if ($Backend -eq "ollama") {
        $body.model = $Model
    }

    $jsonBody = $body | ConvertTo-Json
    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    
    try {
        $uri = "http://127.0.0.1:$Port/api/generate"
        if ($Backend -eq "llama.cpp") {
            $uri = "http://127.0.0.1:$Port/completion"
        }
        $response = Invoke-RestMethod -Uri $uri -Method Post -Body $jsonBody -ContentType "application/json"
    } catch {
        Write-Error "API Error on run ${i}. Check if server is running."
        if ($Backend -eq "llama.cpp") { Stop-Process -Id $serverProcess.Id -Force }
        exit 1
    }
    $stopwatch.Stop()
    
    $duration = $stopwatch.Elapsed.TotalSeconds
    $tokens = 0
    
    if ($Backend -eq "ollama") {
        $tokens = $response.eval_count
    } elseif ($Backend -eq "llama.cpp") {
        $tokens = $response.tokens_predicted
    }

    $speed = $tokens / $duration
    $speeds += $speed
    Write-Host "Run ${i}: $tokens tokens in $duration seconds -> $speed t/s"
}

if ($Backend -eq "llama.cpp") {
    Write-Host "Stopping llama.cpp server..."
    Stop-Process -Id $serverProcess.Id -Force
}

$sorted = $speeds | Sort-Object
$median = 0
if ($sorted.Length % 2 -ne 0) {
    $median = $sorted[[math]::Floor($sorted.Length / 2)]
} else {
    $mid1 = $sorted[$sorted.Length / 2 - 1]
    $mid2 = $sorted[$sorted.Length / 2]
    $median = ($mid1 + $mid2) / 2
}

Write-Host "`n=== BENCHMARK SUMMARY ($Backend) ==="
Write-Host "Total runs: $Iterations"
Write-Host "Median Speed: $median tokens/sec"
