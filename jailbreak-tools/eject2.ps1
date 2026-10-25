$src = @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class DevEject {
    [DllImport("cfgmgr32.dll", SetLastError=true)]
    public static extern int CM_Locate_DevNodeW(out uint devInst, string deviceId, uint flags);
    [DllImport("cfgmgr32.dll", SetLastError=true)]
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

$ids = @(
    'USBSTOR\DISK&VEN_KINDLE&PROD_INTERNAL_STORAGE&REV_0401\G000PP05111409E4&0',
    'USB\VID_1949&PID_0324\G000PP05111409E4'
)
foreach ($id in $ids) {
    Write-Output "Trying: $id"
    $result = [DevEject]::Eject($id)
    Write-Output "  -> $result"
    if ($result -eq 'EJECTED') { break }
}
Start-Sleep -Seconds 3
if (Get-PSDrive -Name D -ErrorAction SilentlyContinue) { Write-Output "STATUS: STILL MOUNTED" } else { Write-Output "STATUS: D: GONE - EJECT CONFIRMED" }
