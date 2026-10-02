# Insert randomized repository watermarks into source and document files.
# Canonical value: GitHub@OxygenAILab | OxygenAILab@StarsailsClover
#
# Rules implemented (BC development watermark specification):
# - one watermark per window of at most 50 non-blank, non-watermark lines;
# - a random eligible position inside each window;
# - random 0-3 spaces inserted after randomly selected characters;
# - never inside string literals, raw strings, code fences, block comments,
#   here-strings, YAML block scalars, or dependency tables.

param(
    [string]$Root = (Split-Path -Parent $PSScriptRoot),
    [string]$Canonical = "GitHub@OxygenAILab | OxygenAILab@StarsailsClover",
    [int]$Window = 50
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$extensions = @(".rs", ".toml", ".md", ".ps1", ".yml", ".yaml")
$excludeDirs = @(
    ".git", "target", ".refers", ".devres", ".devlogs", ".cargo",
    "release", "releases", "node_modules"
)

function Get-CommentLine {
    param([string]$Extension, [string]$Text)
    switch ($Extension) {
        ".rs" { return "// $Text" }
        ".md" { return "<!-- $Text -->" }
        default { return "# $Text" }
    }
}

function New-RandomizedWatermark {
    param([string]$Text)
    $builder = [System.Text.StringBuilder]::new()
    for ($index = 0; $index -lt $Text.Length; $index++) {
        [void]$builder.Append($Text[$index])
        if ((Get-Random -Minimum 0 -Maximum 100) -lt 14) {
            $spaces = Get-Random -Minimum 0 -Maximum 4
            if ($spaces -gt 0) {
                [void]$builder.Append(" " * $spaces)
            }
        }
    }
    return $builder.ToString()
}

function Test-WatermarkLine {
    param([string]$Line)
    $compact = $Line -replace " ", ""
    $target = $Canonical -replace " ", ""
    $trimmed = $Line.TrimStart()
    $isComment = $trimmed.StartsWith("//") -or $trimmed.StartsWith("#") -or $trimmed.StartsWith("<!--")
    return $isComment -and $compact.Contains($target)
}

function Get-SafeBoundaries {
    param([string[]]$Lines, [string]$Extension)

    $safe = New-Object "bool[]" $Lines.Count
    $mode = "normal"
    $rawHashes = 0
    $fenceChar = ""
    $yamlScalarIndent = -1
    $tomlTable = ""

    for ($index = 0; $index -lt $Lines.Count; $index++) {
        $line = $Lines[$index]
        switch ($Extension) {
            ".rs" {
                $position = 0
                while ($position -lt $line.Length) {
                    if ($mode -eq "block") {
                        $close = $line.IndexOf("*/", $position)
                        if ($close -lt 0) { $position = $line.Length; break }
                        $mode = "normal"
                        $position = $close + 2
                        continue
                    }
                    if ($mode -eq "string") {
                        $escaped = $false
                        $closed = $false
                        while ($position -lt $line.Length) {
                            $char = $line[$position]
                            if ($escaped) { $escaped = $false }
                            elseif ($char -eq "\") { $escaped = $true }
                            elseif ($char -eq '"') { $mode = "normal"; $position++; $closed = $true; break }
                            $position++
                        }
                        if (-not $closed) { break }
                        continue
                    }
                    if ($mode -eq "raw") {
                        $needle = '"' + ("#" * $rawHashes)
                        $close = $line.IndexOf($needle, $position)
                        if ($close -lt 0) { $position = $line.Length; break }
                        $mode = "normal"
                        $position = $close + $needle.Length
                        continue
                    }
                    # normal mode: find the next interesting token
                    $lineComment = $line.IndexOf("//", $position)
                    $blockComment = $line.IndexOf("/*", $position)
                    $rawStart = $line.IndexOf("r", $position)
                    $quote = $line.IndexOf('"', $position)
                    $candidates = @(
                        @($lineComment, $blockComment, $rawStart, $quote) |
                            Where-Object { $_ -ge 0 }
                    )
                    if ($candidates.Count -eq 0) { $position = $line.Length; break }
                    $next = ($candidates | Measure-Object -Minimum).Minimum
                    if ($next -eq $lineComment) { $position = $line.Length; break }
                    if ($next -eq $blockComment) { $mode = "block"; $position = $next + 2; continue }
                    if ($next -eq $rawStart) {
                        $match = [regex]::Match($line.Substring($next), '^r(#*)"')
                        if ($match.Success) {
                            $rawHashes = $match.Groups[1].Value.Length
                            $needle = '"' + ("#" * $rawHashes)
                            $close = $line.IndexOf($needle, $next + $match.Length)
                            if ($close -lt 0) { $mode = "raw"; $position = $line.Length; break }
                            $position = $close + $needle.Length
                            continue
                        }
                        $position = $next + 1
                        continue
                    }
                    if ($next -eq $quote) {
                        # Skip character literals so '"' does not toggle string mode.
                        if ($next -gt 0 -and $line[$next - 1] -eq "'" -and $next + 1 -lt $line.Length -and $line[$next + 1] -eq "'") {
                            $position = $next + 2
                            continue
                        }
                        $mode = "string"
                        $position = $next + 1
                        continue
                    }
                    $position = $next + 1
                }
            }
            ".toml" {
                $trimmed = $line.Trim()
                if ($trimmed.StartsWith("[") -and $trimmed.EndsWith("]")) {
                    $tomlTable = $trimmed.Substring(1, $trimmed.Length - 2).ToLowerInvariant()
                }
                if ($mode -eq "basic3") {
                    if ($line.Contains('"""')) { $mode = "normal" }
                }
                elseif ($mode -eq "literal3") {
                    if ($line.Contains("'''")) { $mode = "normal" }
                }
                else {
                    if (($line.Split('"""').Count - 1) % 2 -eq 1) { $mode = "basic3" }
                    elseif (($line.Split("'''").Count - 1) % 2 -eq 1) { $mode = "literal3" }
                }
                $isDependencyTable = $tomlTable -match "^(dependencies|dev-dependencies|build-dependencies|workspace\.dependencies)"
                if ($isDependencyTable) { $safe[$index] = $false; continue }
            }
            ".md" {
                $trimmed = $line.TrimStart()
                if ($mode -eq "fence") {
                    if ($trimmed.StartsWith($fenceChar)) { $mode = "normal" }
                }
                elseif ($mode -eq "comment") {
                    if ($line.Contains("-->")) { $mode = "normal" }
                }
                else {
                    if ($trimmed.StartsWith('```') -or $trimmed.StartsWith('~~~')) {
                        $fenceChar = $trimmed.Substring(0, 3)
                        $mode = "fence"
                    }
                    elseif ($line.Contains("<!--") -and -not $line.Contains("-->")) {
                        $mode = "comment"
                    }
                }
            }
            ".ps1" {
                if ($mode -eq "block") {
                    if ($line.Contains("#>")) { $mode = "normal" }
                }
                elseif ($mode -eq "here-single") {
                    if ($line.TrimStart().StartsWith("'@")) { $mode = "normal" }
                }
                elseif ($mode -eq "here-double") {
                    if ($line.TrimStart().StartsWith('"@')) { $mode = "normal" }
                }
                else {
                    if ($line.TrimStart().StartsWith("<#")) {
                        if (-not $line.Contains("#>")) { $mode = "block" }
                    }
                    elseif ($line -match "@'\s*$") { $mode = "here-single" }
                    elseif ($line -match '@"\s*$') { $mode = "here-double" }
                }
            }
            { $_ -in ".yml", ".yaml" } {
                if ($yamlScalarIndent -ge 0) {
                    if ($line.Trim().Length -eq 0) {
                        # blank lines belong to the scalar
                    }
                    else {
                        $indent = $line.Length - $line.TrimStart().Length
                        if ($indent -le $yamlScalarIndent) { $yamlScalarIndent = -1 }
                    }
                }
                if ($yamlScalarIndent -lt 0 -and $line -match '^(\s*)(-\s+)?[^#:]+:\s*[|>][-+]?\s*(#.*)?$') {
                    $yamlScalarIndent = $Matches[1].Length
                }
                if ($yamlScalarIndent -ge 0) { $safe[$index] = $false; continue }
            }
        }
        $safe[$index] = ($mode -eq "normal")
    }
    return $safe
}

function Add-WatermarksToFile {
    param([string]$Path)

    $extension = [IO.Path]::GetExtension($Path).ToLowerInvariant()
    $raw = [IO.File]::ReadAllText($Path)
    $newline = if ($raw.Contains("`r`n")) { "`r`n" } else { "`n" }
    $lines = [regex]::Split($raw, "`r?`n")
    if ($lines.Count -gt 0 -and $lines[-1] -eq "") {
        if ($lines.Count -eq 1) {
            $lines = @()
        }
        else {
            $lines = $lines[0..($lines.Count - 2)]
        }
    }
    if ($lines.Count -eq 0) { return 0 }

    foreach ($line in $lines) {
        if (Test-WatermarkLine $line) {
            Write-Verbose "skip (already watermarked): $Path"
            return 0
        }
    }

    $safe = Get-SafeBoundaries -Lines $lines -Extension $extension
    $ordinary = @()
    for ($index = 0; $index -lt $lines.Count; $index++) {
        if ($lines[$index].Trim().Length -gt 0) { $ordinary += $index }
    }
    if ($ordinary.Count -eq 0) { return 0 }

    $insertAfter = [System.Collections.Generic.HashSet[int]]::new()
    for ($start = 0; $start -lt $ordinary.Count; $start += $Window) {
        $end = [Math]::Min($start + $Window - 1, $ordinary.Count - 1)
        $candidates = @()
        for ($position = $start; $position -le $end; $position++) {
            $index = $ordinary[$position]
            if (-not $safe[$index]) { continue }
            if ($index + 1 -ge $lines.Count) { continue }
            if ($lines[$index + 1].Trim().Length -eq 0) { continue }
            if ($lines[$index].TrimStart().StartsWith("use ") -or $lines[$index].TrimStart().StartsWith("mod ")) { continue }
            if ($lines[$index + 1].TrimStart().StartsWith("use ") -or $lines[$index + 1].TrimStart().StartsWith("mod ")) { continue }
            $candidates += $index
        }
        if ($candidates.Count -eq 0) {
            Write-Warning "no safe watermark position in window $($start / $Window + 1) of $Path"
            continue
        }
        $pick = $candidates[(Get-Random -Minimum 0 -Maximum $candidates.Count)]
        [void]$insertAfter.Add($pick)
    }
    if ($insertAfter.Count -eq 0) { return 0 }

    $output = [System.Collections.Generic.List[string]]::new()
    for ($index = 0; $index -lt $lines.Count; $index++) {
        $output.Add($lines[$index])
        if ($insertAfter.Contains($index)) {
            $rendered = New-RandomizedWatermark -Text $Canonical
            $output.Add((Get-CommentLine -Extension $extension -Text $rendered))
        }
    }
    $text = [string]::Join($newline, $output) + $newline
    [IO.File]::WriteAllText($Path, $text, [System.Text.UTF8Encoding]::new($false))
    return $insertAfter.Count
}

# Only touch known project files. The workspace may contain untracked scratch
# files from other tools; those must never be modified by this script.
$targetFiles = @(
    "Cargo.toml", "rust-toolchain.toml", "AGENTS.md", "README.md",
    "README.zh-CN.md", "CHANGELOG.md"
)
$targetDirs = @(".devdocs", ".github", "docs", "crates", "scripts")

$files = @()
foreach ($name in $targetFiles) {
    $candidate = Join-Path $Root $name
    if (Test-Path -LiteralPath $candidate -PathType Leaf) {
        $files += Get-Item -LiteralPath $candidate
    }
}
foreach ($dir in $targetDirs) {
    $candidate = Join-Path $Root $dir
    if (Test-Path -LiteralPath $candidate -PathType Container) {
        $files += Get-ChildItem -LiteralPath $candidate -Recurse -File | Where-Object {
            $extensions -contains $_.Extension.ToLowerInvariant() -and
            -not ($_.FullName.Split([IO.Path]::DirectorySeparatorChar) | Where-Object { $excludeDirs -contains $_ })
        }
    }
}

$total = 0
foreach ($file in $files) {
    $count = Add-WatermarksToFile -Path $file.FullName
    if ($count -gt 0) {
        Write-Host ("watermarked {0} ({1})" -f $file.FullName.Substring($Root.Length + 1), $count)
        $total += $count
    }
}
Write-Host "Inserted $total watermark line(s) across $($files.Count) project file(s)."
