param([string]$DriveLetter = "D")
$psd = Get-PSDrive -Name $DriveLetter -ErrorAction Stop
$free = $psd.Free
Write-Output ("Free before: {0} MB" -f [math]::Round($free/1MB))
if ($free -le 90MB) { Write-Output "Storage already below 90 MB free - nothing to do."; exit 0 }

$fillDir = "${DriveLetter}:\fill_disk"
New-Item -ItemType Directory -Path $fillDir -Force | Out-Null

$target = 70MB
$i = 0
while ($i -lt 60) {
    $free = (Get-PSDrive -Name $DriveLetter).Free
    $toFill = $free - $target
    if ($toFill -le 0) { break }
    $chunk = [int64]268435456
    if ($toFill -lt $chunk) { $chunk = [int64]$toFill }
    if ($chunk -lt 1MB) { break }
    $f = Join-Path $fillDir ("filler_{0:d2}.bin" -f $i)
    $fs = [System.IO.File]::Create($f)
    $fs.SetLength($chunk)
    $fs.Close()
    Write-Output ("created {0} ({1} MB)" -f $f, [math]::Round($chunk/1MB))
    $i++
}
$freeFinal = (Get-PSDrive -Name $DriveLetter).Free
Write-Output ("Free after: {0} MB ({1} filler files)" -f [math]::Round($freeFinal/1MB), $i)
if ($freeFinal -ge 50MB -and $freeFinal -le 90MB) { Write-Output "OK: within 50-90 MB target." } else { Write-Output "WARNING: outside 50-90 MB target, check manually." }
