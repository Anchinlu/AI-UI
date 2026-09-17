param(
    [Parameter(Mandatory = $false)][ValidateRange(256, 8192)][int]$NumCtx = 2048,
    [Parameter(Mandatory = $false)][ValidateRange(1, 2048)][int]$NumPredict = 64,
    [Parameter(Mandatory = $false)][ValidateRange(1, 100)][int]$Runs = 5,
    [Parameter(Mandatory = $false)][string]$Model = "",
    [Parameter(Mandatory = $false)][string]$Url = "http://127.0.0.1:11435"
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Model)) {
    $Model = [Environment]::GetEnvironmentVariable("OLLAMA_MODEL")
    if ([string]::IsNullOrWhiteSpace($Model)) {
        $Model = "qwen2.5:1.5b"
    }
}

$OutputFolder = Join-Path $PSScriptRoot "..\..\Docs\benchmarks\raw"
New-Item -ItemType Directory -Force -Path $OutputFolder | Out-Null
$Timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$OutputFile = Join-Path $OutputFolder "context_benchmark_${Timestamp}.csv"
$SummaryFile = Join-Path $OutputFolder "context_benchmark_${Timestamp}_summary.json"

function Get-Median {
    param([double[]]$Values)
    $sorted = @($Values | Sort-Object)
    if ($sorted.Count -eq 0) { return 0.0 }
    if (($sorted.Count % 2) -eq 1) {
        return [double]$sorted[[int][math]::Floor($sorted.Count / 2)]
    }
    $right = [int]($sorted.Count / 2)
    return ([double]$sorted[$right - 1] + [double]$sorted[$right]) / 2.0
}

function Get-OllamaRamMB {
    $processes = @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.ProcessName -like "ollama*" })
    if ($processes.Count -eq 0) { return 0.0 }
    $bytes = ($processes | Measure-Object -Property WorkingSet64 -Sum).Sum
    return [math]::Round(([double]$bytes / 1MB), 2)
}

function Estimate-MessageTokens {
    param([string]$Content)
    return [uint32]([math]::Ceiling($Content.Length / 3.0) + 4)
}

function New-FilledText {
    param([string]$Prefix, [int]$Length)
    $seed = "$Prefix - "
    $builder = New-Object System.Text.StringBuilder
    while ($builder.Length -lt $Length) {
        [void]$builder.Append($seed)
    }
    return $builder.ToString().Substring(0, $Length)
}

function New-Fixture {
    param([int]$Pairs, [int]$CharsPerMessage)
    $messages = @(
        @{ role = "system"; content = "You are a helpful assistant." }
    )
    for ($n = 1; $n -le $Pairs; $n++) {
        $messages += @{ role = "user"; content = New-FilledText "user turn $n" $CharsPerMessage }
        $messages += @{ role = "assistant"; content = New-FilledText "assistant turn $n" $CharsPerMessage }
    }
    $messages += @{ role = "user"; content = "Answer the current question using the most recent context." }
    return ,$messages
}

# The fixtures intentionally include profiles that exceed the 2048 and 8192 budgets.
$Fixtures = [ordered]@{
    "0_Empty" = New-Fixture -Pairs 0 -CharsPerMessage 0
    "1_Short" = New-Fixture -Pairs 1 -CharsPerMessage 120
    "2_Medium" = New-Fixture -Pairs 6 -CharsPerMessage 500
    "3_Long" = New-Fixture -Pairs 16 -CharsPerMessage 1000
}

$csvRows = @()
$summary = @()

Write-Host "AI Taskbar Context Benchmark"
Write-Host "Model: $Model"
Write-Host "URL: $Url"
Write-Host "NumCtx: $NumCtx"
Write-Host "NumPredict: $NumPredict"
Write-Host "Measured runs per fixture: $Runs (plus one warm-up)"
Write-Host "Output: $OutputFile"

foreach ($entry in $Fixtures.GetEnumerator()) {
    $fixtureName = $entry.Key
    $messages = @($entry.Value)
    $estimatedPromptTokens = [uint32](($messages | ForEach-Object { Estimate-MessageTokens $_.content } | Measure-Object -Sum).Sum)
    $body = @{
        model = $Model
        messages = $messages
        stream = $false
        options = @{
            num_ctx = $NumCtx
            num_predict = $NumPredict
            temperature = 0.2
            repeat_penalty = 1.1
        }
    } | ConvertTo-Json -Depth 10 -Compress

    Write-Host "Fixture: $fixtureName (messages=$($messages.Count), estimated_input_tokens=$estimatedPromptTokens)"
    try {
        Invoke-RestMethod -Uri "$Url/api/chat" -Method Post -Body $body -ContentType "application/json" | Out-Null
    } catch {
        throw "Warm-up failed for $fixtureName at $Url/api/chat : $($_.Exception.Message)"
    }

    $tpsValues = @()
    $latencyValues = @()
    $promptCounts = @()
    $ramValues = @()

    for ($i = 1; $i -le $Runs; $i++) {
        $watch = [System.Diagnostics.Stopwatch]::StartNew()
        try {
            $response = Invoke-RestMethod -Uri "$Url/api/chat" -Method Post -Body $body -ContentType "application/json"
        } catch {
            throw "Benchmark failed for $fixtureName run $i at $Url/api/chat : $($_.Exception.Message)"
        } finally {
            $watch.Stop()
        }

        $evalDurationMs = [double]$response.eval_duration / 1000000.0
        $promptEvalDurationMs = [double]$response.prompt_eval_duration / 1000000.0
        $totalDurationMs = [double]$response.total_duration / 1000000.0
        $tokensPerSec = if ($evalDurationMs -gt 0) { ([double]$response.eval_count / $evalDurationMs) * 1000.0 } else { 0.0 }
        $ramMB = Get-OllamaRamMB

        $tpsValues += $tokensPerSec
        $latencyValues += $totalDurationMs
        $promptCounts += [int]$response.prompt_eval_count
        $ramValues += $ramMB
        $csvRows += [pscustomobject]@{
            Timestamp = $Timestamp
            Model = $Model
            Url = $Url
            NumCtx = $NumCtx
            NumPredict = $NumPredict
            HistoryLevel = $fixtureName
            MessageCount = $messages.Count
            EstimatedPromptTokens = $estimatedPromptTokens
            Run = $i
            PromptEvalCount = $response.prompt_eval_count
            PromptEvalDurationMs = $promptEvalDurationMs
            EvalCount = $response.eval_count
            EvalDurationMs = $evalDurationMs
            TotalDurationMs = $totalDurationMs
            TokensPerSec = $tokensPerSec
            OllamaProcessRamMB = $ramMB
        }
        Write-Host ("  Run {0}: prompt={1}, eval={2}, {3:N2} t/s, RAM={4:N2} MB" -f $i,$response.prompt_eval_count,$response.eval_count,$tokensPerSec,$ramMB)
    }

    $summary += [pscustomobject]@{
        Timestamp = $Timestamp
        Model = $Model
        Url = $Url
        NumCtx = $NumCtx
        NumPredict = $NumPredict
        HistoryLevel = $fixtureName
        MeasuredRuns = $Runs
        EstimatedPromptTokens = $estimatedPromptTokens
        MedianTokensPerSec = Get-Median $tpsValues
        MedianLatencyMs = Get-Median $latencyValues
        MedianPromptEvalCount = Get-Median ([double[]]$promptCounts)
        MedianOllamaProcessRamMB = Get-Median $ramValues
    }
}

$csvRows | Export-Csv -Path $OutputFile -NoTypeInformation -Encoding utf8
$summary | ConvertTo-Json -Depth 6 | Set-Content -Path $SummaryFile -Encoding utf8

Write-Host ""
Write-Host "Summary:"
$summary | Format-Table HistoryLevel,NumCtx,EstimatedPromptTokens,MedianTokensPerSec,MedianLatencyMs,MedianOllamaProcessRamMB -AutoSize
Write-Host "Raw CSV: $OutputFile"
Write-Host "Summary JSON: $SummaryFile"
