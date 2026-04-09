# Fix all encoding issues in README.md
$file = "D:\quasar\README.md"
$content = Get-Content $file -Raw -Encoding UTF8

# Replace all broken encodings sequentially
$content = $content -replace 'â€"','—'
$content = $content -replace 'â€"','–'
$content = $content -replace 'â†"','↔'
$content = $content -replace 'Ã—','×'
$content = $content -replace 'ðŸ¦€','🦀'
$content = $content -replace 'ðŸš€','🚀'
$content = $content -replace 'â€"','—'
$content = $content -replace 'â€"','–'

Set-Content $file -Value $content -Encoding UTF8 -NoNewline
Write-Host "All encoding issues fixed in README.md"
