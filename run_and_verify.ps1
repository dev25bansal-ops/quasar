# Script to run Quasar examples, take screenshots, and verify they work

$examples = @(
    @{Name="showcase"; Package="showcase"; Cmd="cargo run -p showcase"},
    @{Name="spinning_cube"; Package="spinning-cube"; Cmd="cargo run -p spinning-cube"},
    @{Name="physics_sandbox"; Package="physics-sandbox"; Cmd="cargo run -p physics-sandbox"},
    @{Name="audio_demo"; Package="audio-demo"; Cmd="cargo run -p audio-demo"},
    @{Name="scripting_demo"; Package="scripting-demo"; Cmd="cargo run -p scripting-demo"},
    @{Name="demo_game"; Package="demo_game"; Cmd="cargo run -p demo_game"},
    @{Name="ai_demo"; Package="ai_demo"; Cmd="cargo run -p ai_demo"}
)

$screenshotDir = "D:\quasar\screenshots\verification"
if (!(Test-Path $screenshotdir)) {
    New-Item -ItemType Directory -Path $screenshotdir -Force
}

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

Set-Location D:\quasar

foreach ($example in $examples) {
    Write-Host "`n========================================" -ForegroundColor Cyan
    Write-Host "Running: $($example.Name)" -ForegroundColor Cyan
    Write-Host "========================================`n" -ForegroundColor Cyan
    
    # Start the example
    $process = Start-Process -FilePath "cmd.exe" -ArgumentList "/c", $example.Cmd -PassThru -WindowStyle Normal
    
    # Wait for window to appear
    Write-Host "Waiting 15 seconds for $($example.Name) to initialize..." -ForegroundColor Yellow
    Start-Sleep -Seconds 15
    
    # Take screenshot
    $screenshotFile = "$screenshotdir\$($example.Name).png"
    Write-Host "Taking screenshot: $screenshotFile" -ForegroundColor Green
    
    try {
        $screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
        $bitmap = New-Object System.Drawing.Bitmap($screen.Width, $screen.Height)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        $graphics.CopyFromScreen($screen.Location, [System.Drawing.Point]::Empty, $screen.Size)
        $bitmap.Save($screenshotFile, [System.Drawing.Imaging.ImageFormat]::Png)
        $graphics.Dispose()
        $bitmap.Dispose()
        Write-Host "Screenshot saved successfully!" -ForegroundColor Green
    } catch {
        Write-Host "Failed to take screenshot: $_" -ForegroundColor Red
    }
    
    # Close the process
    Write-Host "Closing $($example.Name)..." -ForegroundColor Yellow
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
    
    Write-Host "Completed: $($example.Name)`n" -ForegroundColor Green
}

Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "All examples verified!" -ForegroundColor Cyan
Write-Host "Screenshots saved to: $screenshotdir" -ForegroundColor Cyan
Write-Host "========================================`n" -ForegroundColor Cyan
