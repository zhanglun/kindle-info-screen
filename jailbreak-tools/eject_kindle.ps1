Write-VolumeCache -DriveLetter D
Write-Output "Volume cache flushed."
$shell = New-Object -ComObject Shell.Application
$drive = $shell.NameSpace(17).ParseName("D:\")
$ejected = $false
foreach ($v in $drive.Verbs()) {
    $name = ($v.Name -replace '&','')
    if ($name -match '弹出|Eject|拔') {
        Write-Output "Invoking verb: $name"
        $v.DoIt()
        $ejected = $true
        break
    }
}
if (-not $ejected) {
    Write-Output "No localized eject verb found; trying Eject by name."
    $drive.InvokeVerb("Eject")
}
Start-Sleep -Seconds 3
$still = Get-PSDrive -Name D -ErrorAction SilentlyContinue
if ($still) { Write-Output "WARNING: D: still mounted." } else { Write-Output "OK: Kindle safely ejected." }
