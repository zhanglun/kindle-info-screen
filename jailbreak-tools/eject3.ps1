$src = @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class DevEject2 {
    [DllImport("cfgmgr32.dll", SetLastError=true, CharSet=CharSet.Unicode)]
    public static extern int CM_Locate_DevNodeW(out uint devInst, string deviceId, uint flags);
    [DllImport("cfgmgr32.dll", SetLastError=true, CharSet=CharSet.Unicode)]
    public static extern int CM_Request_Device_EjectW(uint devInst, IntPtr pVetoType, StringBuilder vetoName, uint nameLength, uint flags);
    public static string Eject(string deviceId) {
        uint devInst;
        int cr = CM_Locate_DevNodeW(out devInst, deviceId, 0);
        if (cr != 0) { return "Locate failed, CR=" + cr; }
        StringBuilder veto = new StringBuilder(512);
        cr = CM_Request_Device_EjectW(devInst, IntPtr.Zero, veto, 512u, 0);
        if (cr == 0) { return "EJECTED"; }
        return "Eject failed, CR=" + cr + " veto=" + veto.ToString();
    }
}
"@
Add-Type -TypeDefinition $src -Language CSharp

$targets = Get-PnpDevice -PresentOnly | Where-Object { $_.InstanceId -match 'USBSTOR\\DISK&VEN_KINDLE' -or $_.InstanceId -match '^USB\\VID_1949' }
foreach ($t in $targets) {
    Write-Output "Ejecting: $($t.InstanceId)"
    $result = [DevEject2]::Eject($t.InstanceId)
    Write-Output "  -> $result"
    if ($result -eq 'EJECTED') { break }
}
Start-Sleep -Seconds 4
$kindleStill = Get-PnpDevice -PresentOnly | Where-Object { $_.InstanceId -match 'USBSTOR\\DISK&VEN_KINDLE' }
$dExists = Test-Path "D:\"
if (-not $kindleStill -and -not $dExists) { Write-Output "STATUS: EJECT CONFIRMED (no Kindle storage device, no D:)" }
else { Write-Output "STATUS: KindleStoragePresent=$([bool]$kindleStill) DExists=$dExists" }
