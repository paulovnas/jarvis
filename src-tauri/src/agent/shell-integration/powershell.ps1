$global:__JarvisToken = '__JARVIS_TOKEN__'
$global:__JarvisActive = $false
function global:__JarvisEncode([string] $Value) {
    [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($Value))
}
function global:__JarvisEmit([string] $Kind, [string] $Value) {
    $cwd = __JarvisEncode $ExecutionContext.SessionState.Path.CurrentFileSystemLocation.Path
    $prefix = "$([char]27)]777;jarvis;$global:__JarvisToken;$Kind"
    if ($Kind -eq 'start') { $prefix += ';' + $cwd + ';' + (__JarvisEncode $Value) }
    elseif ($Kind -eq 'end') { $prefix += ';' + $Value + ';' + $cwd }
    elseif ($Kind -eq 'idle') {
        $jobs = @(Get-Job -State Running -ErrorAction SilentlyContinue).Count
        $prefix += ';' + $cwd + ';' + $jobs
    }
    [Console]::Write($prefix + [char]7)
}
$readLine = Get-Command PSConsoleHostReadLine -CommandType Function -ErrorAction SilentlyContinue
if (-not $readLine) {
    Import-Module PSReadLine -ErrorAction SilentlyContinue
    $readLine = Get-Command PSConsoleHostReadLine -CommandType Function -ErrorAction SilentlyContinue
}
$originalPrompt = Get-Command prompt -CommandType Function -ErrorAction SilentlyContinue
if ($readLine -and $originalPrompt) {
    $global:__JarvisReadLine = $readLine.ScriptBlock
    $global:__JarvisPrompt = $originalPrompt.ScriptBlock
    function global:PSConsoleHostReadLine {
        $line = & $global:__JarvisReadLine
        if (-not [string]::IsNullOrWhiteSpace($line)) {
            $global:__JarvisActive = $true
            __JarvisEmit start $line
        }
        return $line
    }
    function global:prompt {
        $success = $?
        $exitCode = $global:LASTEXITCODE
        if ($global:__JarvisActive) {
            $global:__JarvisActive = $false
            if ($success) { $exitCode = 0 }
            elseif (-not $exitCode) { $exitCode = 1 }
            __JarvisEmit end ([string]$exitCode)
        }
        __JarvisEmit idle ''
        & $global:__JarvisPrompt
    }
}
